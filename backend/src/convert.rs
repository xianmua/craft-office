//! 旧版二进制 Office 格式转换：检测到 LibreOffice（soffice）时，把 doc/xls/ppt/csv
//! 转成对应 OOXML 再交给内核加载；未安装 soffice 时上层保持原有「不支持」拒绝行为。
//! 转换产物只用于展示/编辑新格式，不回写原文档。

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;
use tokio::sync::Mutex;

/// 转换超时：soffice 冷启动 + 大文档可能较慢
const CONVERT_TIMEOUT: Duration = Duration::from_secs(120);

/// 旧格式 → OOXML 目标扩展名；非旧格式返回 None
pub fn target_ext(ext: &str) -> Option<&'static str> {
    match ext {
        "doc" => Some("docx"),
        "xls" => Some("xlsx"),
        "ppt" => Some("pptx"),
        "csv" => Some("xlsx"),
        _ => None,
    }
}

static SOFFICE: OnceLock<Option<PathBuf>> = OnceLock::new();
/// soffice 同一 profile 不能并发跑多实例，转换全局串行
static CONVERT_LOCK: Mutex<()> = Mutex::const_new(());

/// soffice 可执行文件路径（进程内缓存一次）：环境变量 CRAFT_SOFFICE → vendor/libreoffice 便携版 → PATH → 常见安装路径
pub fn soffice_path() -> Option<&'static PathBuf> {
    SOFFICE
        .get_or_init(|| {
            if let Ok(p) = std::env::var("CRAFT_SOFFICE") {
                let p = PathBuf::from(p);
                if p.is_file() {
                    return Some(p);
                }
                tracing::warn!("CRAFT_SOFFICE 指向的文件不存在：{p:?}，回退自动探测");
            }
            find_soffice().inspect(|p| {
                tracing::info!(
                    "检测到 LibreOffice：{}，旧格式（doc/xls/ppt/csv）将自动转换为 OOXML",
                    p.display()
                )
            })
        })
        .as_ref()
}

fn find_soffice() -> Option<PathBuf> {
    let exe = if cfg!(windows) { "soffice.exe" } else { "soffice" };
    // 项目自带便携版优先：vendor/libreoffice/（免安装，不入库，拷过去即用）
    if let Some(p) = vendor_soffice(exe) {
        return Some(p);
    }
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            let p = dir.join(exe);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    common_locations(exe).into_iter().find(|p| p.is_file())
}

/// vendor/libreoffice 便携版探测：cwd 向上回溯 5 级找 vendor/libreoffice，
/// 兼容 backend/ 直接运行、仓库根运行与发布包目录运行；布局支持
/// `program/soffice(.exe)` 或其下任意一级子目录的 `program/`（官方便携版解压即用）。
fn vendor_soffice(exe: &str) -> Option<PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
    for _ in 0..5 {
        let base = dir.join("vendor").join("libreoffice");
        let direct = base.join("program").join(exe);
        if direct.is_file() {
            return Some(direct);
        }
        if let Ok(entries) = std::fs::read_dir(&base) {
            for e in entries.flatten() {
                let p = e.path().join("program").join(exe);
                if p.is_file() {
                    return Some(p);
                }
            }
        }
        dir = dir.parent()?.to_path_buf();
    }
    None
}

#[cfg(windows)]
fn common_locations(exe: &str) -> Vec<PathBuf> {
    [
        "C:\\Program Files\\LibreOffice\\program",
        "C:\\Program Files (x86)\\LibreOffice\\program",
    ]
    .iter()
    .map(|d| Path::new(d).join(exe))
    .collect()
}

#[cfg(not(windows))]
fn common_locations(exe: &str) -> Vec<PathBuf> {
    ["/usr/local/bin", "/opt/libreoffice/program", "/usr/bin"]
        .iter()
        .map(|d| Path::new(d).join(exe))
        .collect()
}

/// 转换旧格式字节为 OOXML；成功返回（目标扩展名, 转换后字节）
pub async fn convert(ext: &str, bytes: Vec<u8>) -> Result<(String, Vec<u8>), String> {
    let Some(target) = target_ext(ext) else {
        return Err(format!(".{ext} 不是可转换的旧格式"));
    };
    let Some(soffice) = soffice_path() else {
        return Err("未检测到 LibreOffice（soffice），无法自动转换旧格式".to_string());
    };

    let _guard = CONVERT_LOCK.lock().await;

    // 工作目录与 profile 固定复用（转换已全局串行）：profile 重建约 15s 冷启动，
    // 只清输入/输出文件，首次之后每次转换仅 2-5s
    let dir = std::env::temp_dir().join("artcraft-convert");
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建临时目录失败：{e}"))?;
    let src = dir.join(format!("input.{ext}"));
    let result = run_convert(soffice, &dir, &src, target, bytes).await;
    let _ = std::fs::remove_file(&src);
    let _ = std::fs::remove_file(dir.join(format!("input.{target}")));
    result
}

async fn run_convert(
    soffice: &Path,
    dir: &Path,
    src: &Path,
    target: &str,
    bytes: Vec<u8>,
) -> Result<(String, Vec<u8>), String> {
    std::fs::write(src, bytes).map_err(|e| format!("写入临时文件失败：{e}"))?;

    // 独立 profile：避免与宿主上正在运行的 LibreOffice 实例冲突（该参数要求 file:// URI）
    let profile = file_uri(&dir.join("profile"));
    let mut child = tokio::process::Command::new(soffice)
        .arg(format!("-env:UserInstallation={profile}"))
        .args(["--headless", "--norestore", "--convert-to", target, "--outdir"])
        .arg(dir)
        .arg(src)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("启动 soffice 失败：{e}"))?;

    let status = match tokio::time::timeout(CONVERT_TIMEOUT, child.wait()).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => return Err(format!("等待 soffice 失败：{e}")),
        Err(_) => {
            let _ = child.kill().await;
            return Err("soffice 转换超时（120s）".to_string());
        }
    };
    if !status.success() {
        return Err(format!(
            "soffice 转换失败（退出码 {}），文件可能已损坏或不是有效的旧格式文档",
            status.code().unwrap_or(-1)
        ));
    }

    let dest = dir.join(format!("input.{target}"));
    let bytes = std::fs::read(&dest).map_err(|_| "转换完成但未生成输出文件".to_string())?;
    Ok((target.to_string(), bytes))
}

/// 路径 → file:// URI（soffice -env 参数要求；非保留字符以外的字节按 UTF-8 百分号编码）
fn file_uri(p: &Path) -> String {
    let raw = p.to_string_lossy().replace('\\', "/");
    let mut out = if raw.starts_with('/') {
        String::from("file://")
    } else {
        String::from("file:///")
    };
    for b in raw.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' | b':' => {
                out.push(*b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_legacy_targets() {
        assert_eq!(target_ext("doc"), Some("docx"));
        assert_eq!(target_ext("xls"), Some("xlsx"));
        assert_eq!(target_ext("ppt"), Some("pptx"));
        assert_eq!(target_ext("csv"), Some("xlsx"));
        assert_eq!(target_ext("docx"), None);
        assert_eq!(target_ext("exe"), None);
    }

    #[test]
    fn builds_file_uri() {
        #[cfg(windows)]
        assert_eq!(
            file_uri(Path::new("C:\\Temp dir\\profile")),
            "file:///C:/Temp%20dir/profile"
        );
        #[cfg(not(windows))]
        assert_eq!(
            file_uri(Path::new("/tmp/a b/profile")),
            "file:///tmp/a%20b/profile"
        );
    }
}
