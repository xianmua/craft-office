//! 第三方文件拉取接口（拉模式）：前端每次保存把最新文档字节上报到后端内存，
//! 第三方系统凭令牌随时来拉取，编辑器页面不注入任何第三方回调脚本。
//!
//! 端点（均需令牌，query `?token=` 或请求头 `X-Save-Token`）：
//!   POST /api/save/latest        上报最新保存（前端自动调用；body 为文档原始字节）
//!   GET  /api/save/latest        元信息 {name, saveAs, savedAt, size}
//!   GET  /api/save/latest/file   文档字节（attachment 下载）
//!
//! 安全：SAVE_API_TOKEN 未配置或为空时全部端点 403 禁用（默认关闭）；
//! 字节仅存内存最新一份（覆盖式，防止累积），进程重启即清。

use std::sync::{Arc, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use actix_web::{get, post, web, HttpRequest, HttpResponse};

use crate::config::AppConfig;
use crate::routes::doc::content_disposition;

#[derive(Clone)]
struct LastSave {
    name: String,
    save_as: bool,
    bytes: Vec<u8>,
    saved_at: u64,
}

static LAST: OnceLock<Mutex<Option<LastSave>>> = OnceLock::new();

fn store() -> &'static Mutex<Option<LastSave>> {
    LAST.get_or_init(|| Mutex::new(None))
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 令牌校验：SAVE_API_TOKEN 未配置/为空 → 接口禁用；配置了则请求必须带来相同令牌。
/// 仅用于拉取端点（GET）；上报端点不设 token——token 无法安全进前端产物（公开即失效），
/// 且上报的数据本就是编辑器保存出来准备交付第三方的，敏感出口在拉取侧。
fn token_ok(req: &HttpRequest, q_token: Option<&String>) -> Result<(), HttpResponse> {
    let expected = std::env::var("SAVE_API_TOKEN").ok().unwrap_or_default();
    if expected.trim().is_empty() {
        return Err(HttpResponse::Forbidden().json(serde_json::json!({
            "code": "save_api_disabled",
            "message": "保存拉取接口未启用：需在 .env 配置 SAVE_API_TOKEN"
        })));
    }
    let got = req
        .headers()
        .get("X-Save-Token")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .or_else(|| q_token.cloned());
    match got {
        Some(t) if t == expected => Ok(()),
        _ => Err(HttpResponse::Forbidden().json(serde_json::json!({
            "code": "bad_token",
            "message": "令牌缺失或不正确"
        }))),
    }
}

#[derive(serde::Deserialize)]
pub struct SaveQuery {
    pub name: Option<String>,
    #[serde(default)]
    pub save_as: bool,
    pub token: Option<String>,
}

/// 上报最新保存：body 为文档原始字节（前端保存时自动调用；不设 token，见 token_ok 注释）
#[post("/api/save/latest")]
pub async fn push_latest(
    q: web::Query<SaveQuery>,
    body: web::Bytes,
    cfg: web::Data<Arc<AppConfig>>,
) -> HttpResponse {
    if body.is_empty() {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "code": "empty_body", "message": "缺少文档字节"
        }));
    }
    if body.len() as u64 > cfg.doc.max_bytes {
        return HttpResponse::PayloadTooLarge().json(serde_json::json!({
            "code": "too_large", "message": format!("文档超过大小上限 {} bytes", cfg.doc.max_bytes)
        }));
    }
    let name = q
        .name
        .clone()
        .unwrap_or_else(|| "document".to_string())
        .replace(['"', '\\', '/', '\r', '\n', '\0'], "");
    let name = if name.is_empty() { "document".to_string() } else { name };
    let size = body.len();
    *store().lock().unwrap_or_else(|e| e.into_inner()) = Some(LastSave {
        name,
        save_as: q.save_as,
        bytes: body.to_vec(),
        saved_at: now_secs(),
    });
    HttpResponse::Ok().json(serde_json::json!({ "ok": true, "size": size }))
}

/// 最新保存元信息
#[get("/api/save/latest")]
pub async fn latest_meta(req: HttpRequest, q: web::Query<SaveQuery>) -> HttpResponse {
    if let Err(resp) = token_ok(&req, q.token.as_ref()) {
        return resp;
    }
    let guard = store().lock().unwrap_or_else(|e| e.into_inner());
    match guard.as_ref() {
        Some(s) => HttpResponse::Ok().json(serde_json::json!({
            "name": s.name, "saveAs": s.save_as, "savedAt": s.saved_at, "size": s.bytes.len()
        })),
        None => HttpResponse::NotFound().json(serde_json::json!({
            "code": "no_save", "message": "暂无保存记录"
        })),
    }
}

/// 最新保存文档字节（attachment 下载）
#[get("/api/save/latest/file")]
pub async fn latest_file(req: HttpRequest, q: web::Query<SaveQuery>) -> HttpResponse {
    if let Err(resp) = token_ok(&req, q.token.as_ref()) {
        return resp;
    }
    let guard = store().lock().unwrap_or_else(|e| e.into_inner());
    match guard.as_ref() {
        Some(s) => HttpResponse::Ok()
            .insert_header(("Content-Type", "application/octet-stream"))
            .insert_header(("Content-Disposition", content_disposition("attachment", &s.name)))
            .insert_header(("Cache-Control", "no-store"))
            .body(s.bytes.clone()),
        None => HttpResponse::NotFound().json(serde_json::json!({
            "code": "no_save", "message": "暂无保存记录"
        })),
    }
}
