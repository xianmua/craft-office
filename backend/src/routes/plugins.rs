//! 插件系统：两个目录、三种动作。
//! - dev_dir（项目级，只读）：每插件一目录（manifest.json + 插件 JS），调试改完即生效，默认启用；
//!   「卸载」对项目级插件只是写入屏蔽记录（enabled=false），不删文件。
//! - data_dir（用户级，可写）：「安装」= 把插件目录整份拷入这里；「卸载」= 删整个目录。
//! - installed.json 存在 data_dir 根部（[{id, enabled}]），是启停状态的唯一事实源；未记录的插件默认启用。
//! 接口形状不变：GET /api/plugins 返回 {market, installed}，宿主按 enabled 过滤后经
//! /api/plugins/file?id= 拉取插件 JS 执行。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use actix_web::{get, post, web, HttpResponse};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::config::AppConfig;
use crate::error::ApiError;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MarketEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    /// 插件入口 JS（相对插件目录）
    pub file: String,
    /// 来源：project（dev_dir）| user（data_dir）
    #[serde(default)]
    pub source: String,
}

#[derive(Debug, Deserialize)]
pub struct PluginOp {
    pub id: String,
}

/// 启停操作：内核插件管理器/市场面板调用。
#[derive(Debug, Deserialize)]
pub struct EnableOp {
    pub id: String,
    pub enabled: bool,
}

/// installed.json 的条目：兼容旧格式（纯 id 字符串数组，缺省视为启用）。
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InstalledEntry {
    pub id: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

fn read_manifest(plugin_dir: &Path, source: &str) -> Option<MarketEntry> {
    let raw = std::fs::read_to_string(plugin_dir.join("manifest.json")).ok()?;
    let mut entry: MarketEntry = serde_json::from_str(&raw).ok()?;
    entry.source = source.to_string();
    Some(entry)
}

/// 扫一个插件根目录下的所有 <id>/manifest.json。
fn scan_plugins(dir: &Path, source: &str) -> Vec<MarketEntry> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut list = Vec::new();
    for item in entries.flatten() {
        let path = item.path();
        if !path.is_dir() {
            continue;
        }
        if let Some(entry) = read_manifest(&path, source) {
            // 目录名即插件 id，manifest 里的 id 仅作展示校验
            if entry.id == path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default() {
                list.push(entry);
            }
        }
    }
    list.sort_by(|a, b| a.id.cmp(&b.id));
    list
}

fn dev_dir(cfg: &AppConfig) -> PathBuf {
    Path::new(&cfg.plugins.dev_dir).to_path_buf()
}

fn data_dir(cfg: &AppConfig) -> PathBuf {
    Path::new(&cfg.plugins.data_dir).to_path_buf()
}

fn installed_path(cfg: &AppConfig) -> PathBuf {
    data_dir(cfg).join("installed.json")
}

/// 全量可用插件：项目级优先，同 id 用户级被覆盖。
fn all_plugins(cfg: &AppConfig) -> Result<Vec<MarketEntry>, ApiError> {
    let mut list = scan_plugins(&dev_dir(cfg), "project");
    for entry in scan_plugins(&data_dir(cfg), "user") {
        if !list.iter().any(|m| m.id == entry.id) {
            list.push(entry);
        }
    }
    Ok(list)
}

fn read_installed(cfg: &AppConfig) -> Vec<InstalledEntry> {
    let raw = match std::fs::read_to_string(installed_path(cfg)) {
        Ok(raw) => raw,
        Err(_) => return Vec::new(),
    };
    let value: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    match value {
        serde_json::Value::Array(items) => items
            .into_iter()
            .filter_map(|item| match item {
                serde_json::Value::String(id) => Some(InstalledEntry { id, enabled: true }),
                v => serde_json::from_value(v).ok(),
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// 原子写入：先写临时文件再改名，避免并发写坏 installed.json。
fn write_installed(cfg: &AppConfig, entries: &[InstalledEntry]) -> Result<(), ApiError> {
    let path = installed_path(cfg);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ApiError::Internal(format!("创建插件目录失败：{e}")))?;
    }
    let raw = serde_json::to_string_pretty(entries).map_err(|e| ApiError::Internal(format!("序列化失败：{e}")))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, raw).map_err(|e| ApiError::Internal(format!("写入 installed.json 失败：{e}")))?;
    std::fs::rename(&tmp, &path).map_err(|e| ApiError::Internal(format!("替换 installed.json 失败：{e}")))
}

/// 递归拷贝插件目录（安装 = 整目录拷入用户级）。
fn copy_dir(src: &Path, dst: &Path) -> Result<(), ApiError> {
    std::fs::create_dir_all(dst).map_err(|e| ApiError::Internal(format!("创建目录失败：{e}")))?;
    for item in std::fs::read_dir(src).map_err(|e| ApiError::Internal(format!("读取目录失败：{e}")))?.flatten() {
        let from = item.path();
        let to = dst.join(item.file_name());
        if from.is_dir() {
            copy_dir(&from, &to)?;
        } else {
            std::fs::copy(&from, &to).map_err(|e| ApiError::Internal(format!("拷贝 {} 失败：{e}", item.file_name().to_string_lossy())))?;
        }
    }
    Ok(())
}

/// 市场列表 + 已安装状态；插件 JS 单独经 /api/plugins/file 下发。
#[get("/api/plugins")]
pub async fn list_plugins(cfg: web::Data<Arc<AppConfig>>) -> Result<HttpResponse, ApiError> {
    let market = all_plugins(&cfg)?;
    let state = read_installed(&cfg);
    let enabled_of = |id: &str| state.iter().find(|s| s.id == id).map(|s| s.enabled).unwrap_or(true);
    let installed: Vec<InstalledEntry> = market
        .iter()
        .map(|m| InstalledEntry { id: m.id.clone(), enabled: enabled_of(&m.id) })
        .collect();
    Ok(HttpResponse::Ok().json(json!({ "market": market, "installed": installed })))
}

#[post("/api/plugins/install")]
pub async fn install_plugin(op: web::Json<PluginOp>, cfg: web::Data<Arc<AppConfig>>) -> Result<HttpResponse, ApiError> {
    let id = op.id.trim();
    if !valid_id(id) {
        return Err(ApiError::Internal("非法插件 id".to_string()));
    }
    let plugins = all_plugins(&cfg)?;
    let entry = plugins
        .iter()
        .find(|m| m.id == id)
        .ok_or_else(|| ApiError::Internal(format!("市场里没有插件：{id}")))?;
    // 项目级插件拷入用户级（持久化，脱离项目也能用）；用户级插件安装即无操作
    if entry.source == "project" {
        let src = dev_dir(&cfg).join(id);
        let dst = data_dir(&cfg).join(id);
        if dst.exists() {
            std::fs::remove_dir_all(&dst).map_err(|e| ApiError::Internal(format!("清理旧版本失败：{e}")))?;
        }
        copy_dir(&src, &dst)?;
    }
    let mut state = read_installed(&cfg);
    if !state.iter().any(|x| x.id == id) {
        state.push(InstalledEntry { id: id.to_string(), enabled: true });
    } else if let Some(item) = state.iter_mut().find(|x| x.id == id) {
        item.enabled = true; // 重装即重新启用
    }
    write_installed(&cfg, &state)?;
    Ok(HttpResponse::Ok().json(json!({ "ok": true })))
}

#[post("/api/plugins/uninstall")]
pub async fn uninstall_plugin(op: web::Json<PluginOp>, cfg: web::Data<Arc<AppConfig>>) -> Result<HttpResponse, ApiError> {
    let id = op.id.trim();
    if !valid_id(id) {
        return Err(ApiError::Internal("非法插件 id".to_string()));
    }
    let plugins = all_plugins(&cfg)?;
    let present = plugins.iter().find(|m| m.id == id).map(|m| m.source.clone());
    // 无论来源，用户级副本一律删除；项目级本体保留，仅写屏蔽记录
    let dst = data_dir(&cfg).join(id);
    if dst.exists() {
        std::fs::remove_dir_all(&dst).map_err(|e| ApiError::Internal(format!("删除插件目录失败：{e}")))?;
    }
    let state: Vec<InstalledEntry> = read_installed(&cfg)
        .into_iter()
        .filter(|x| x.id != id)
        .collect();
    let mut state = state;
    if present.is_some() && present.as_deref() != Some("user") {
        state.push(InstalledEntry { id: id.to_string(), enabled: false });
    }
    write_installed(&cfg, &state)?;
    Ok(HttpResponse::Ok().json(json!({ "ok": true })))
}

/// 启用/禁用插件；内核经 Services::plugin_load 重新装载。
#[post("/api/plugins/enable")]
pub async fn enable_plugin(op: web::Json<EnableOp>, cfg: web::Data<Arc<AppConfig>>) -> Result<HttpResponse, ApiError> {
    let id = op.id.trim();
    if !valid_id(id) {
        return Err(ApiError::Internal("非法插件 id".to_string()));
    }
    let plugins = all_plugins(&cfg)?;
    if !plugins.iter().any(|m| m.id == id) {
        return Err(ApiError::Internal(format!("插件不存在：{id}")));
    }
    let mut state = read_installed(&cfg);
    if let Some(entry) = state.iter_mut().find(|x| x.id == id) {
        entry.enabled = op.enabled;
    } else {
        state.push(InstalledEntry { id: id.to_string(), enabled: op.enabled });
    }
    write_installed(&cfg, &state)?;
    Ok(HttpResponse::Ok().json(json!({ "ok": true })))
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && !id.contains('/') && !id.contains('\\') && !id.contains("..")
}

/// 下发插件 JS（text/javascript），供宿主页面加载执行；项目级优先。
#[get("/api/plugins/file")]
pub async fn plugin_file(q: web::Query<PluginOp>, cfg: web::Data<Arc<AppConfig>>) -> Result<HttpResponse, ApiError> {
    let id = q.id.trim();
    if id.is_empty() || id.contains('/') || id.contains('\\') || id.contains("..") {
        return Err(ApiError::Internal("非法插件 id".to_string()));
    }
    let plugins = all_plugins(&cfg)?;
    let entry = plugins
        .iter()
        .find(|m| m.id == id)
        .ok_or_else(|| ApiError::Internal(format!("市场里没有插件：{id}")))?;
    let base = if entry.source == "project" { dev_dir(&cfg) } else { data_dir(&cfg) };
    let file = base.join(id).join(&entry.file);
    let code = std::fs::read_to_string(file)
        .map_err(|e| ApiError::Internal(format!("读取插件 {id} 失败：{e}")))?;
    Ok(HttpResponse::Ok()
        .content_type("text/javascript; charset=utf-8")
        .body(code))
}
