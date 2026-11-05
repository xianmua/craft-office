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
}

#[derive(Debug, Clone, Deserialize)]
pub struct AllowHost {
    pub host: String,
    #[serde(default = "default_paths")]
    pub paths: Vec<String>,
}

/// 插件市场：目录里 market.json 是上架目录，installed.json 记录已安装。
#[derive(Debug, Clone, Deserialize)]
pub struct PluginsConfig {
    #[serde(default = "default_plugins_dir")]
    pub dir: String,
}

impl Default for PluginsConfig {
    fn default() -> Self {
        Self { dir: default_plugins_dir() }
    }
}

fn default_plugins_dir() -> String {
    "./plugins".to_string()
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
        let cfg: AppConfig = toml::from_str(&raw).map_err(|e| format!("配置解析失败：{e}"))?;
        if cfg.doc.allow_hosts.is_empty() {
            return Err("doc.allow_hosts 为空：代理会被拒绝所有域名，请至少配置一个白名单域名".to_string());
        }
        Ok(cfg)
    }
}
