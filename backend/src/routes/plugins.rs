//! 插件市场（MVP）：后端只管「目录 + 已安装清单 + 插件文件下发」。
//! 插件本体是在宿主页面执行的 JS（拿一个 hostApi），市场面板负责列出与安装。

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
    /// 插件 JS 文件名（相对插件目录）
    pub file: String,
}

#[derive(Debug, Deserialize)]
pub struct PluginOp {
    pub id: String,
}

fn plugins_dir(cfg: &AppConfig) -> std::path::PathBuf {
    std::path::Path::new(&cfg.plugins.dir).to_path_buf()
}

fn market_path(cfg: &AppConfig) -> std::path::PathBuf {
    plugins_dir(cfg).join("market.json")
}

fn installed_path(cfg: &AppConfig) -> std::path::PathBuf {
    plugins_dir(cfg).join("installed.json")
}

fn read_market(cfg: &AppConfig) -> Result<Vec<MarketEntry>, ApiError> {
    let raw = std::fs::read_to_string(market_path(cfg))
        .map_err(|e| ApiError::Internal(format!("读取 market.json 失败：{e}")))?;
    serde_json::from_str(&raw).map_err(|e| ApiError::Internal(format!("market.json 解析失败：{e}")))
}

fn read_installed(cfg: &AppConfig) -> Vec<String> {
    std::fs::read_to_string(installed_path(cfg))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn write_installed(cfg: &AppConfig, ids: &[String]) -> Result<(), ApiError> {
    let path = installed_path(cfg);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ApiError::Internal(format!("创建插件目录失败：{e}")))?;
    }
    let raw = serde_json::to_string_pretty(ids).map_err(|e| ApiError::Internal(format!("序列化失败：{e}")))?;
    std::fs::write(path, raw).map_err(|e| ApiError::Internal(format!("写入 installed.json 失败：{e}")))
}

/// 市场列表 + 已安装状态；插件 JS 单独经 /api/plugins/file 下发。
#[get("/api/plugins")]
pub async fn list_plugins(cfg: web::Data<Arc<AppConfig>>) -> Result<HttpResponse, ApiError> {
    let market = read_market(&cfg)?;
    let installed = read_installed(&cfg);
    Ok(HttpResponse::Ok().json(json!({ "market": market, "installed": installed })))
}

#[post("/api/plugins/install")]
pub async fn install_plugin(op: web::Json<PluginOp>, cfg: web::Data<Arc<AppConfig>>) -> Result<HttpResponse, ApiError> {
    let id = op.id.trim();
    if id.is_empty() || id.contains('/') || id.contains('\\') || id.contains("..") {
        return Err(ApiError::Internal("非法插件 id".to_string()));
    }
    let market = read_market(&cfg)?;
    if !market.iter().any(|m| m.id == id) {
        return Err(ApiError::Internal(format!("市场里没有插件：{id}")));
    }
    let mut installed = read_installed(&cfg);
    if !installed.iter().any(|x| x == id) {
        installed.push(id.to_string());
        write_installed(&cfg, &installed)?;
    }
    Ok(HttpResponse::Ok().json(json!({ "ok": true, "installed": installed })))
}

#[post("/api/plugins/uninstall")]
pub async fn uninstall_plugin(op: web::Json<PluginOp>, cfg: web::Data<Arc<AppConfig>>) -> Result<HttpResponse, ApiError> {
    let id = op.id.trim();
    let installed: Vec<String> = read_installed(&cfg).into_iter().filter(|x| x != id).collect();
    write_installed(&cfg, &installed)?;
    Ok(HttpResponse::Ok().json(json!({ "ok": true, "installed": installed })))
}

/// 下发插件 JS（text/javascript），供宿主页面加载执行。
#[get("/api/plugins/file")]
pub async fn plugin_file(q: web::Query<PluginOp>, cfg: web::Data<Arc<AppConfig>>) -> Result<HttpResponse, ApiError> {
    let id = q.id.trim();
    if id.is_empty() || id.contains('/') || id.contains('\\') || id.contains("..") {
        return Err(ApiError::Internal("非法插件 id".to_string()));
    }
    let market = read_market(&cfg)?;
    let entry = market
        .iter()
        .find(|m| m.id == id)
        .ok_or_else(|| ApiError::Internal(format!("市场里没有插件：{id}")))?;
    let file = plugins_dir(&cfg).join(&entry.file);
    let code = std::fs::read_to_string(file)
        .map_err(|e| ApiError::Internal(format!("读取插件 {id} 失败：{e}")))?;
    Ok(HttpResponse::Ok()
        .content_type("text/javascript; charset=utf-8")
        .body(code))
}
