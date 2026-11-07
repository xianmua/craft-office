use actix_files::NamedFile;
use actix_web::{get, web, HttpRequest, HttpResponse, Result as ActixResult};
use std::path::Path;
use std::str::FromStr;
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

#[derive(serde::Deserialize)]
struct ProductQuery {
    url: Option<String>,
}

/// 产品页：/wordcraft?url=xxx.docx 直接以全屏内核打开远程文档。
/// 动态输出一层极薄宿主页（全屏 iframe 加载内核 + /api/doc/proxy 桥），内核零改动。
/// gridcraft 等后续内核接入时复用（同一桥协议，仅前缀不同）。
#[get("/wordcraft")]
pub async fn wordcraft_page(
    req: HttpRequest,
    cfg: web::Data<Arc<AppConfig>>,
) -> ActixResult<HttpResponse> {
    let q = web::Query::<ProductQuery>::from_query(req.query_string())
        .map_err(|e| actix_web::error::ErrorBadRequest(format!("参数解析失败：{e}")))?;
    match q.url.as_deref().map(str::trim).filter(|u| !u.is_empty()) {
        Some(url) => Ok(html_response(product_host_html("wordcraft", url))),
        // 无 url：回退静态内核页（独立完整编辑器）
        None => {
            let cfg: &Arc<AppConfig> = &cfg;
            Ok(html_response(index_html(cfg.as_ref()).await?))
        }
    }
}

/// gridcraft 产品页：与 wordcraft 同一桥协议，仅前缀不同。
#[get("/gridcraft")]
pub async fn gridcraft_page(
    req: HttpRequest,
    cfg: web::Data<Arc<AppConfig>>,
) -> ActixResult<HttpResponse> {
    let q = web::Query::<ProductQuery>::from_query(req.query_string())
        .map_err(|e| actix_web::error::ErrorBadRequest(format!("参数解析失败：{e}")))?;
    match q.url.as_deref().map(str::trim).filter(|u| !u.is_empty()) {
        Some(url) => Ok(html_response(product_host_html("gridcraft", url))),
        None => {
            let cfg: &Arc<AppConfig> = &cfg;
            Ok(html_response(index_html(cfg.as_ref()).await?))
        }
    }
}

/// 产品宿主页模板。KERNEL 前缀决定 iframe 路径与桥消息类型。
fn product_host_html(kernel: &str, url: &str) -> String {
    let kernel_json = serde_json::to_string(kernel).unwrap_or_else(|_| "\"wordcraft\"".into());
    let url_json = serde_json::to_string(url).unwrap_or_else(|_| "\"\"".into());

    format!(
        r#"<!DOCTYPE html>
<html lang="zh">
<head>
<meta charset="utf-8">
<title>{title}</title>
<style>
html,body{{margin:0;height:100%;background:#1a1a1a;font-family:system-ui,sans-serif}}
#stage{{width:100vw;height:100vh}}
iframe{{border:0;width:100%;height:100%;display:block}}
#status{{position:fixed;inset:0;display:flex;align-items:center;justify-content:center;color:#9aa0a6;font-size:14px}}
.spin{{width:28px;height:28px;border:3px solid #3a3a3a;border-top-color:#7aa2ff;border-radius:50%;animation:spin 1s linear infinite;margin-bottom:12px}}
@keyframes spin{{to{{transform:rotate(360deg)}}}}</style>
</head>
<body>
<div id="stage"></div>
<div id="status"><div style="display:flex;flex-direction:column;align-items:center"><div class="spin"></div><div>正在通过后端代理拉取文档…</div></div></div>
<script>
(function() {{
"use strict";
var KERNEL = {kernel};
var DOC_URL = {url};
var stage = document.getElementById('stage');
var status = document.getElementById('status');
var iframe = document.createElement('iframe');
iframe.src = '/' + KERNEL + '/?host=parent';
stage.appendChild(iframe);

function hideStatus() {{ status.style.display = 'none'; }}
function showErr(m) {{ status.innerHTML = '<div style="max-width:30em;line-height:1.9;color:#e06c75">' + m + '</div>'; }}
function nameOf(u) {{ try {{ var n = decodeURIComponent(u.split('/').pop().split('?')[0]); return n || 'document'; }} catch (e) {{ return 'document'; }} }}
function openInKernel(buf, name) {{ iframe.contentWindow.postMessage({{ type: KERNEL + ':open', name: name, bytes: buf }}, '*'); }}

var pending = null, ready = false;
window.addEventListener('message', function(ev) {{
  var d = ev.data || {{}};
  if (d.type === KERNEL + ':ready') {{
    ready = true;
    if (pending) {{ openInKernel(pending.bytes, pending.name); pending = null; hideStatus(); }}
  }} else if (d.type === KERNEL + ':save') {{
    if (d.bytes) {{
      var blob = new Blob([d.bytes], {{ type: 'application/octet-stream' }});
      var a = document.createElement('a');
      a.href = URL.createObjectURL(blob);
      a.download = (d.name && String(d.name).trim()) || nameOf(DOC_URL);
      a.click();
    }}
  }} else if (d.type === KERNEL + ':failed') {{
    showErr('内核加载失败：' + (d.error || '未知错误'));
  }}
}});

fetch('/api/doc/proxy?url=' + encodeURIComponent(DOC_URL))
  .then(function(r) {{ if (!r.ok) throw new Error('HTTP ' + r.status); return r.arrayBuffer(); }})
  .then(function(buf) {{
    var name = nameOf(DOC_URL);
    if (ready) {{ openInKernel(buf, name); hideStatus(); }}
    else {{ pending = {{ name: name, bytes: buf }}; }}
  }})
  .catch(function(e) {{ showErr('文档拉取失败：' + e.message + '<br>请检查链接是否可访问。'); }});
}})();
</script>
</body>
</html>
"#,
        title = html_escape(kernel),
        kernel = kernel_json,
        url = url_json,
    )
}

fn html_escape(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '<' => "&lt;".to_string(),
            '>' => "&gt;".to_string(),
            '&' => "&amp;".to_string(),
            '"' => "&quot;".to_string(),
            c => c.to_string(),
        })
        .collect()
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

/// ArtCraft 内核静态资源：显式指定 MIME。
/// .wasm 必须返回 application/wasm，否则 instantiateStreaming 会直接失败。
#[get("/artcraft/{file}")]
pub async fn artcraft_file(
    req: HttpRequest,
    path: web::Path<String>,
    cfg: web::Data<Arc<AppConfig>>,
) -> ActixResult<HttpResponse> {
    let name = path.into_inner();

    // 只允许单段、安全字符的文件名（防止 ../ 与特殊字符）
    let safe = !name.is_empty()
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));

    if !safe {
        return Err(actix_web::error::ErrorNotFound("资源不存在"));
    }

    let cfg: &Arc<AppConfig> = &cfg;
    let full = Path::new(&cfg.server.static_dir).join("artcraft").join(&name);

    let file = NamedFile::open_async(&full)
        .await
        .map_err(|_| actix_web::error::ErrorNotFound("内核产物不存在，请按 vendor/artcraft/README.md 放置文件"))?;

    let ctype = if name.ends_with(".wasm") {
        "application/wasm"
    } else if name.ends_with(".js") {
        "text/javascript; charset=utf-8"
    } else {
        "application/octet-stream"
    };

    // set_content_type 是「消费 self 并返回 Self」的写法，不能当 &mut 用
    let file = match mime::Mime::from_str(ctype) {
        Ok(mime) => file.set_content_type(mime),
        Err(_) => file,
    };

    Ok(file.into_response(&req))
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
