use actix_web::{get, web, HttpRequest, HttpResponse, Result as ActixResult};
use std::path::Path;
use std::sync::Arc;

use crate::config::AppConfig;

const INDEX_CACHE: &str = "no-store";

async fn index_html(cfg: &AppConfig) -> ActixResult<String> {
    let path = Path::new(&cfg.server.static_dir).join("index.html");
    tokio::fs::read_to_string(&path).await.map_err(|e| {
        actix_web::error::ErrorServiceUnavailable(format!(
            "前端产物缺失（{}）：请先在 frontend 目录执行 npm run build（{e}）",
            path.display()
        ))
    })
}

fn html_response(html: String) -> HttpResponse {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .insert_header(("Cache-Control", INDEX_CACHE))
        .body(html)
}

#[get("/")]
pub async fn office_root(cfg: web::Data<Arc<AppConfig>>) -> ActixResult<HttpResponse> {
    let cfg: &Arc<AppConfig> = &cfg;
    Ok(html_response(index_html(cfg.as_ref()).await?))
}

/// 健康检查：顺便告诉前端 / 运维内核产物是否就位。
#[get("/api/health")]
pub async fn health(cfg: web::Data<Arc<AppConfig>>) -> ActixResult<HttpResponse> {
    let cfg: &Arc<AppConfig> = &cfg;
    let kernel_dir = Path::new(&cfg.server.static_dir).join("wordcraft");
    // 内核就位 = index.html + 任一 *_bg.wasm（trunk build 产物，文件名带哈希）
    let kernel_wasm = std::fs::read_dir(&kernel_dir)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .any(|e| e.file_name().to_string_lossy().ends_with("_bg.wasm"))
        })
        .unwrap_or(false);
    let kernel_ready = kernel_wasm && kernel_dir.join("index.html").exists();

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "ok": true,
        "service": "artcraft-office",
        "version": env!("CARGO_PKG_VERSION"),
        "editor_kernel_ready": kernel_ready,
        "max_bytes": cfg.doc.max_bytes,
        "allow_hosts": cfg.doc.allow_hosts.iter().map(|h| h.host.clone()).collect::<Vec<_>>(),
    })))
}

/// SPA 兜底：非 /api 路径都返回 index.html，交给前端路由。
pub async fn spa_fallback(
    req: HttpRequest,
    cfg: web::Data<Arc<AppConfig>>,
) -> ActixResult<HttpResponse> {
    if req.path().starts_with("/api/") {
        return Ok(HttpResponse::NotFound().json(serde_json::json!({
            "code": "not_found",
            "message": "接口不存在"
        })));
    }
    let cfg: &Arc<AppConfig> = &cfg;
    Ok(html_response(index_html(cfg.as_ref()).await?))
}
