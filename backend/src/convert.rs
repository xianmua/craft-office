//! 旧版二进制 Office 格式转换：双引擎，优先 Node sidecar（vendor/convert-svc/，
//! 基于 @deepseek-ai/libreoffice-kit，更快且免装 soffice），失败回退 LibreOffice
//! （soffice）；把 doc/xls/ppt/csv 转成对应 OOXML 再交给内核加载。两个引擎都
//! 不可用时上层保持原有「不支持」拒绝行为。转换产物只用于展示/编辑新格式，不回写原文档。

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;
use tokio::io::AsyncBufReadExt;
use tokio::sync::Mutex;

/// 转换超时：soffice 冷启动 + 大文档可能较慢
const CONVERT_TIMEOUT: Duration = Duration::from_secs(120);
/// sidecar 启动等待 PORT 行的超时
const SIDECAR_START_TIMEOUT: Duration = Duration::from_secs(30);

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

/// 是否存在至少一个可用转换引擎（sidecar 或 soffice），供 supported/converted 上报
pub fn convert_available() -> bool {
    sidecar_available() || soffice_path().is_some()
}

/// sidecar 可用性（纯文件检查，进程内缓存）：vendor/convert-svc/svc.js + node
pub fn sidecar_available() -> bool {
    sidecar_dir().map(|d| find_node(d).is_some()).unwrap_or(false)
}

static SOFFICE: OnceLock<Option<PathBuf>> = OnceLock::new();
/// soffice 同一 profile 不能并发跑多实例，且 sidecar 引擎内部也是串行，转换全局串行
static CONVERT_LOCK: Mutex<()> = Mutex::const_new(());

// ---------------------------------------------------------------------------
// 引擎 1：Node sidecar（vendor/convert-svc/）
// ---------------------------------------------------------------------------

static SIDECAR_DIR: OnceLock<Option<PathBuf>> = OnceLock::new();

/// sidecar 目录：cwd 向上回溯 5 级找 vendor/convert-svc/svc.js（与 vendor_soffice 同模式）
fn sidecar_dir() -> Option<&'static PathBuf> {
    SIDECAR_DIR
        .get_or_init(|| {
            let mut dir = std::env::current_dir().ok()?;
            for _ in 0..5 {
                let d = dir.join("vendor").join("convert-svc");
                if d.join("svc.js").is_file() {
                    return Some(d);
                }
                dir = dir.parent()?.to_path_buf();
            }
            None
        })
        .as_ref()
}

/// node 可执行文件：sidecar 目录内自带的 node.exe（发布包）→ PATH
fn find_node(dir: &Path) -> Option<PathBuf> {
    let exe = if cfg!(windows) { "node.exe" } else { "node" };
    let local = dir.join(exe);
    if local.is_file() {
        return Some(local);
    }
    if let Some(paths) = std::env::var_os("PATH") {
        for d in std::env::split_paths(&paths) {
            let p = d.join(exe);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

struct Sidecar {
    port: u16,
    /// 保活句柄：持有 stdin 使 svc.js 检测父进程存活；drop 前由 reset 显式 kill
    child: tokio::process::Child,
    _stdin: tokio::process::ChildStdin,
}

enum SidecarState {
    Off,
    Running(Box<Sidecar>),
    /// 启动类硬失败（spawn 失败 / 等不到 PORT），进程生命周期内不再重试
    Failed,
}

static SIDECAR: Mutex<SidecarState> = Mutex::const_new(SidecarState::Off);
static SIDECAR_HTTP: OnceLock<reqwest::Client> = OnceLock::new();

fn sidecar_http() -> &'static reqwest::Client {
    SIDECAR_HTTP.get_or_init(|| reqwest::Client::new())
}

/// 懒启动 sidecar：spawn node svc.js，读 stdout 等 `PORT=` 行
async fn sidecar_start() -> Result<Sidecar, String> {
    let dir = sidecar_dir().ok_or("未找到 vendor/convert-svc")?;
    let node = find_node(dir).ok_or("未找到 node 可执行文件")?;

    let mut child = tokio::process::Command::new(node)
        .arg(dir.join("svc.js"))
        .current_dir(dir)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("启动 convert-svc 失败：{e}"))?;

    let stdin = child.stdin.take().ok_or("convert-svc stdin 不可用")?;
    let stdout = child.stdout.take().ok_or("convert-svc stdout 不可用")?;

    let port = tokio::time::timeout(SIDECAR_START_TIMEOUT, async {
        let mut lines = tokio::io::BufReader::new(stdout).lines();
        while let Some(line) = lines.next_line().await.map_err(|e| e.to_string())? {
            if let Some(p) = line.strip_prefix("PORT=") {
                return p.parse::<u16>().map_err(|e| e.to_string());
            }
        }
        Err("convert-svc 提前退出".to_string())
    })
    .await
    .map_err(|_| {
        let _ = child.start_kill();
        "等待 convert-svc 端口超时（30s）".to_string()
    })??;

    tracing::info!("convert-svc 已启动（127.0.0.1:{port}），旧格式转换走 sidecar 引擎");
    Ok(Sidecar { port, child, _stdin: stdin })
}

/// 停掉 sidecar 子进程并回到 Off 状态
async fn sidecar_reset(state: &mut SidecarState) {
    if let SidecarState::Running(mut sc) = std::mem::replace(state, SidecarState::Off) {
        let _ = sc.child.start_kill();
        let _ = sc.child.wait().await;
    }
}

enum SidecarOutcome {
    /// sidecar 未配置或已判定启动失败：静默走 soffice
    Missing,
    /// sidecar 配置了但本次转换失败（已回退 soffice，若其存在）
    Failed(String),
    Ok(Vec<u8>),
}

async fn sidecar_convert(ext: &str, target: &str, bytes: &[u8]) -> SidecarOutcome {
    if !sidecar_available() {
        return SidecarOutcome::Missing;
    }
    let mut guard = SIDECAR.lock().await;
    if matches!(*guard, SidecarState::Failed) {
        return SidecarOutcome::Missing;
    }
    if !matches!(*guard, SidecarState::Running(_)) {
        match sidecar_start().await {
            Ok(sc) => *guard = SidecarState::Running(Box::new(sc)),
            Err(e) => {
                tracing::warn!("convert-svc 启动失败，本进程内回退 LibreOffice：{e}");
                *guard = SidecarState::Failed;
                return SidecarOutcome::Failed(e);
            }
        }
    }
    let SidecarState::Running(sc) = &*guard else {
        return SidecarOutcome::Missing;
    };
    let url = format!("http://127.0.0.1:{}/convert?from={ext}&to={target}", sc.port);

    let resp = sidecar_http()
        .post(&url)
        .timeout(CONVERT_TIMEOUT)
        .body(bytes.to_vec())
        .send()
        .await;
    match resp {
        Ok(r) if r.status().is_success() => match r.bytes().await {
            Ok(b) => SidecarOutcome::Ok(b.to_vec()),
            Err(e) => {
                // 读响应中断：服务可能已挂，重置后下次请求重新拉起
                sidecar_reset(&mut guard).await;
                SidecarOutcome::Failed(format!("读取 sidecar 响应失败：{e}"))
            }
        },
        Ok(r) => {
            // 服务活着但转换失败（多为文档损坏）：保留 sidecar，仅本次回退
            let status = r.status().as_u16();
            let msg = r.text().await.unwrap_or_default();
            SidecarOutcome::Failed(format!("sidecar 转换失败（HTTP {status}）：{msg}"))
        }
        Err(e) => {
            sidecar_reset(&mut guard).await;
            SidecarOutcome::Failed(format!("请求 sidecar 失败：{e}"))
        }
    }
}

// ---------------------------------------------------------------------------
// 引擎 2：LibreOffice soffice（兜底）
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// 入口
// ---------------------------------------------------------------------------

/// 转换旧格式字节为 OOXML；成功返回（目标扩展名，转换后字节）
pub async fn convert(ext: &str, bytes: Vec<u8>) -> Result<(String, Vec<u8>), String> {
    let Some(target) = target_ext(ext) else {
        return Err(format!(".{ext} 不是可转换的旧格式"));
    };
    let _guard = CONVERT_LOCK.lock().await;

    // 引擎 1：Node sidecar
    match sidecar_convert(ext, target, &bytes).await {
        SidecarOutcome::Ok(out) => return Ok((target.to_string(), out)),
        SidecarOutcome::Missing => {}
        SidecarOutcome::Failed(e) => {
            if soffice_path().is_none() {
                return Err(format!("旧格式转换失败（convert-svc）：{e}"));
            }
            tracing::warn!("convert-svc 转换失败，回退 LibreOffice：{e}");
        }
    }

    // 引擎 2：soffice 兜底
    let Some(soffice) = soffice_path() else {
        return Err("未检测到转换引擎：convert-svc 与 LibreOffice 均不可用".to_string());
    };

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
