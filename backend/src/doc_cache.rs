//! 文档磁盘缓存：慢上游（境外源站）同一 URL 只慢第一人，之后秒开。
//! key = SHA-256(url)，文件名 `{hash}.{ext}`；写入走 `{hash}.{ext}.{唯一后缀}.part`
//! 临时文件，流式传输完整结束后原子改名提交，中断即丢弃。

use sha2::{Digest, Sha256};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static SEQ: AtomicU64 = AtomicU64::new(0);

pub fn key_of(url: &str, ext: &str) -> String {
    let hash = format!("{:x}", Sha256::digest(url.as_bytes()));
    if ext.is_empty() {
        hash
    } else {
        format!("{hash}.{ext}")
    }
}

pub struct DocCache {
    dir: PathBuf,
}

impl DocCache {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// 命中返回缓存文件路径；目录不存在/文件不存在返回 None。
    pub fn get(&self, key: &str) -> Option<PathBuf> {
        let p = self.dir.join(key);
        p.is_file().then_some(p)
    }

    /// 创建唯一 .part 临时文件（父目录按需创建）；顺带清理超过 1 小时的残留 .part
    /// （客户端中途断开时 actix 直接 drop 流，来不及走 abort 清理）。
    pub fn part_file(&self, key: &str) -> io::Result<(PathBuf, fs::File)> {
        fs::create_dir_all(&self.dir)?;
        if let Ok(entries) = fs::read_dir(&self.dir) {
            let stale = std::time::Duration::from_secs(3600);
            for e in entries.flatten() {
                if e.path().extension().and_then(|x| x.to_str()) == Some("part") {
                    if let Ok(meta) = e.metadata() {
                        if let Ok(m) = meta.modified() {
                            if m.elapsed().map(|age| age > stale).unwrap_or(false) {
                                let _ = fs::remove_file(e.path());
                            }
                        }
                    }
                }
            }
        }
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let part = self.dir.join(format!("{key}.{}.part", n));
        let f = fs::File::create(&part)?;
        Ok((part, f))
    }

    /// 传输完整结束后提交；目标已存在（并发同 URL）时静默放弃。
    pub fn commit(&self, part: &Path, key: &str) {
        let dest = self.dir.join(key);
        if fs::rename(part, &dest).is_err() {
            let _ = fs::remove_file(part);
        }
    }

    /// 传输中断：丢弃半成品。
    pub fn abort(&self, part: &Path) {
        let _ = fs::remove_file(part);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_is_stable_and_suffixes_ext() {
        let k1 = key_of("https://a/b.docx", "docx");
        let k2 = key_of("https://a/b.docx", "docx");
        assert_eq!(k1, k2);
        assert!(k1.ends_with(".docx"));
        assert_eq!(key_of("https://a/b.docx", "xlsx"), key_of("https://a/b.docx", "xlsx"));
        assert_ne!(k1, key_of("https://a/b.docx", "xlsx"));
        // 无扩展名 URL 也能生成 key
        assert_eq!(key_of("https://a/b", "").len(), 64);
        assert_ne!(key_of("https://a/b", ""), key_of("https://a/c", ""));
    }
}
