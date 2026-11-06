use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub doc: DocConfig,
    #[serde(default)]
    pub plugins: PluginsConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub bind: String,
    #[serde(default)]
    pub public_base_url: String,
    #[serde(default = "default_static_dir")]
    pub static_dir: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DocConfig {
    #[serde(default = "default_max_bytes")]
    pub max_bytes: u64,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    #[serde(default = "default_allow_scheme")]
    pub allow_scheme: Vec<String>,
    #[serde(default = "default_allow_ports")]
    pub allow_ports: Vec<u16>,
    #[serde(default)]
    pub allow_hosts: Vec<AllowHost>,
    /// 文档磁盘缓存目录（同 URL 二次访问直接回源缓存，不再拉上游）
    #[serde(default = "default_cache_dir")]
    pub cache_dir: String,
    /// 本地示例文档目录（?url=/samples/xxx.docx 直接读这里，不出网）
    #[serde(default = "default_samples_dir")]
    pub samples_dir: String,
}

fn default_cache_dir() -> String {
    "./data/doc-cache".to_string()
}
fn default_samples_dir() -> String {
    "./samples".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct AllowHost {
    pub host: String,
    #[serde(default = "default_paths")]
    pub paths: Vec<String>,
}

/// 插件目录：dev_dir 是项目级插件（只读、进 git，调试改完即生效）；
/// data_dir 是用户级安装目录（可写，安装 = 整目录拷入，卸载 = 删目录），installed.json 也在这里。
#[derive(Debug, Clone, Deserialize)]
pub struct PluginsConfig {
    #[serde(default = "default_plugins_dev_dir")]
    pub dev_dir: String,
    #[serde(default = "default_plugins_data_dir")]
    pub data_dir: String,
}

impl Default for PluginsConfig {
    fn default() -> Self {
        Self { dev_dir: default_plugins_dev_dir(), data_dir: default_plugins_data_dir() }
    }
}

fn default_plugins_dev_dir() -> String {
    "../plugins".to_string()
}

fn default_plugins_data_dir() -> String {
    "~/.craft-office/plugins".to_string()
}

/// 展开路径前导 ~（Windows 用 USERPROFILE，其余回落 HOME）。
fn expand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
        if let Ok(home) = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
            return format!("{home}/{rest}");
        }
    }
    path.to_string()
}

fn default_static_dir() -> String {
    "./static".to_string()
}
fn default_max_bytes() -> u64 {
    50 * 1024 * 1024
}
fn default_timeout_secs() -> u64 {
    30
}
fn default_allow_scheme() -> Vec<String> {
    vec!["https".to_string()]
}
fn default_allow_ports() -> Vec<u16> {
    vec![443]
}
fn default_paths() -> Vec<String> {
    vec!["/*".to_string()]
}

impl AppConfig {
    /// 从 TOML 文件加载配置。解析失败时明确报错，不做静默降级。
    pub fn load(path: &str) -> Result<Self, String> {
        let p = Path::new(path);
        if !p.exists() {
            return Err(format!("配置文件不存在：{path}"));
        }
        let raw = std::fs::read_to_string(p).map_err(|e| format!("读取配置失败：{e}"))?;
        let mut cfg: AppConfig = toml::from_str(&raw).map_err(|e| format!("配置解析失败：{e}"))?;
        cfg.plugins.dev_dir = expand_tilde(&cfg.plugins.dev_dir);
        cfg.plugins.data_dir = expand_tilde(&cfg.plugins.data_dir);
        if cfg.doc.allow_hosts.is_empty() {
            return Err("doc.allow_hosts 为空：代理会被拒绝所有域名，请至少配置一个白名单域名".to_string());
        }
        Ok(cfg)
    }
}
