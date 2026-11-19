mod config;
mod convert;
mod doc_cache;
mod error;
mod extract;
mod guard;
mod routes;

use actix_files as fs;
use actix_web::{middleware, web, App, HttpServer};
use std::sync::Arc;

use crate::config::AppConfig;
use crate::guard::UrlGuard;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let cfg_path = std::env::var("ARTCRAFT_CONFIG").unwrap_or_else(|_| "config.toml".to_string());
    let cfg = match AppConfig::load(&cfg_path) {
        Ok(c) => Arc::new(c),
        Err(e) => {
            eprintln!("[fatal] 配置加载失败（{cfg_path}）：{e}");
            std::process::exit(1);
        }
    };
    let guard = Arc::new(UrlGuard::from_config(&cfg.doc));

    let bind = std::env::var("ARTCRAFT_BIND").unwrap_or_else(|_| cfg.server.bind.clone());
    tracing::info!(
        "ArtCraft Office 启动，监听 {bind}（对外 {}），静态目录 {}，白名单 {} 个域名",
        cfg.server.public_base_url,
        cfg.server.static_dir,
        cfg.doc.allow_hosts.len()
    );

    let cfg_data = web::Data::new(cfg.clone());
    let guard_data = web::Data::new(guard.clone());
    let static_dir = cfg.server.static_dir.clone();

    HttpServer::new(move || {
        let assets_dir = std::path::Path::new(&static_dir).join("assets");
        App::new()
            .app_data(cfg_data.clone())
            .app_data(guard_data.clone())
            // 保存上报 body 上限放宽到文档大小上限（默认 2MB 会拦住大文档）
            .app_data(web::PayloadConfig::new(cfg_data.doc.max_bytes as usize))
            .wrap(middleware::Logger::default())
            .wrap(middleware::Compress::default())
            // 前端产物静态资源（哈希文件名，可长缓存）
            .service(
                fs::Files::new("/assets", assets_dir)
                    .prefer_utf8(true)
                    .use_etag(true),
            )
            // CJK 兜底字体（backend/static/fonts，见 docs/BUILD.md；外壳 font.add 自动安装）
            .service(
                fs::Files::new("/fonts", std::path::Path::new(&static_dir).join("fonts"))
                    .prefer_utf8(true)
                    .use_etag(true),
            )
            // 内核静态资源（wordcraft/gridcraft/deckcraft trunk 产物；统一入口是 /office?url=）
            .service(
                fs::Files::new("/gridcraft", std::path::Path::new(&static_dir).join("gridcraft"))
                    .index_file("index.html")
                    .prefer_utf8(true)
                    .use_etag(true)
                    .redirect_to_slash_directory(),
            )
            .service(
                fs::Files::new("/deckcraft", std::path::Path::new(&static_dir).join("deckcraft"))
                    .index_file("index.html")
                    .prefer_utf8(true)
                    .use_etag(true)
                    .redirect_to_slash_directory(),
            )
            .service(
                fs::Files::new("/wordcraft", std::path::Path::new(&static_dir).join("wordcraft"))
                    .index_file("index.html")
                    .prefer_utf8(true)
                    .use_etag(true)
                    .redirect_to_slash_directory(),
            )
            .service(routes::doc::doc_meta)
            .service(routes::doc::doc_proxy)
            .service(routes::save_store::push_latest)
            .service(routes::save_store::latest_meta)
            .service(routes::save_store::latest_file)
            .service(routes::ai::ai_chat)
            .service(routes::ai::ai_config_get)
            .service(routes::ai::ai_config_put)
            .service(routes::mcp::mcp_post)
            .service(routes::mcp::mcp_hint)
            .service(routes::plugins::list_plugins)
            .service(routes::plugins::install_plugin)
            .service(routes::plugins::uninstall_plugin)
            .service(routes::plugins::enable_plugin)
            .service(routes::plugins::plugin_file)
            .service(routes::plugins::skill_file)
            .service(routes::plugins::resource_file)
            .service(routes::page::health)
            .service(routes::page::office_root)
            .default_service(web::to(routes::page::spa_fallback))
    })
    .bind(bind.as_str())?
    .run()
    .await
}
