//! MCP（Model Context Protocol）HTTP 端点：无状态 JSON-RPC 2.0 over POST /mcp。
//! 让外部 Agent（Claude / DeepSeek 客户端 / 任意 MCP 宿主）能调用本服务的能力：
//!   - list_allowed_hosts：当前代理白名单
//!   - doc_meta(url)：探测远程文档元信息（走 SSRF 防护）
//!   - doc_text(url)：拉取远程文档并抽取纯文本（走 SSRF 防护）
//!
//! 无状态单 POST 实现 MCP 的 initialize / tools/list / tools/call，
//! 兼容「Streamable HTTP」的最小用法（不需要 SSE 与会话保持）。

use actix_web::{get, post, web, HttpResponse};
use serde_json::{json, Value};
use std::sync::Arc;

use crate::config::AppConfig;
use crate::error::ApiError;
use crate::extract;
use crate::guard::UrlGuard;
use crate::routes::doc::{self, Purpose};

const PROTOCOL_VERSION: &str = "2024-11-05";

#[get("/mcp")]
pub async fn mcp_hint() -> HttpResponse {
    HttpResponse::MethodNotAllowed().json(json!({
        "code": "method_not_allowed",
        "message": "MCP 端点请用 POST /mcp，JSON-RPC 2.0（initialize / tools/list / tools/call）"
    }))
}

#[post("/mcp")]
pub async fn mcp_post(
    body: web::Json<Value>,
    guard: web::Data<Arc<UrlGuard>>,
    cfg: web::Data<Arc<AppConfig>>,
) -> HttpResponse {
    let req = body.into_inner();
    let method = req.get("method").and_then(Value::as_str).unwrap_or("");
    let id = req.get("id").cloned();
    let params = req.get("params").cloned().unwrap_or(json!({}));

    // 通知（无 id）不回包
    if id.is_none() {
        return HttpResponse::Accepted().finish();
    }

    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "artcraft-office-mcp", "version": env!("CARGO_PKG_VERSION") }
        })),
        "tools/list" => Ok(tools_list()),
        "tools/call" => tools_call(&params, &guard, &cfg).await,
        "ping" => Ok(json!({})),
        other => Err(json!({
            "code": -32601,
            "message": format!("未知方法：{other}")
        })),
    };

    match result {
        Ok(result) => HttpResponse::Ok().json(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result
        })),
        Err(error) => HttpResponse::Ok().json(json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": error
        })),
    }
}

fn tools_list() -> Value {
    json!({
        "tools": [
            {
                "name": "list_allowed_hosts",
                "description": "列出文档代理允许访问的域名白名单（含路径前缀限制）",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "doc_meta",
                "description": "探测白名单内远程 Office 文档的元信息（扩展名、大小、是否受支持）",
                "inputSchema": {
                    "type": "object",
                    "properties": { "url": { "type": "string", "description": "文档直链（https）" } },
                    "required": ["url"]
                }
            },
            {
                "name": "doc_text",
                "description": "拉取白名单内远程 docx / xlsx / pptx 并抽取纯文本（上限 20 万字符），供阅读与分析",
                "inputSchema": {
                    "type": "object",
                    "properties": { "url": { "type": "string", "description": "文档直链（https）" } },
                    "required": ["url"]
                }
            }
        ]
    })
}

async fn tools_call(
    params: &Value,
    guard: &UrlGuard,
    cfg: &AppConfig,
) -> Result<Value, Value> {
    let name = params.get("name").and_then(Value::as_str).unwrap_or("");
    let args = params.get("arguments").cloned().unwrap_or(json!({}));
    let text = match name {
        "list_allowed_hosts" => {
            let lines = cfg
                .doc
                .allow_hosts
                .iter()
                .map(|h| {
                    let paths = if h.paths.is_empty() { "*".to_string() } else { h.paths.join(", ") };
                    format!("{}（路径：{paths}）", h.host)
                })
                .collect::<Vec<_>>()
                .join("\n");
            Ok(format!("当前白名单（{} 个域名）：\n{lines}", cfg.doc.allow_hosts.len()))
        }
        "doc_meta" => {
            let url = arg_str(&args, "url")?;
            let prep = doc::prepare(cfg, guard, &url, Purpose::Meta)
                .await
                .map_err(api_err)?;
            let ext = doc::detect_ext(prep.target.path());
            let size = head_size(&prep).await;
            Ok(format!(
                "ext={ext} supported={} size={}",
                matches!(ext.as_str(), "docx" | "xlsx" | "pptx"),
                size.map(|s| s.to_string()).unwrap_or_else(|| "unknown".into())
            ))
        }
        "doc_text" => {
            let url = arg_str(&args, "url")?;
            let prep = doc::prepare(cfg, guard, &url, Purpose::Stream)
                .await
                .map_err(api_err)?;
            let ext = doc::detect_ext(prep.target.path());
            let resp = prep
                .client
                .get(prep.target.clone())
                .header(reqwest::header::ACCEPT_ENCODING, "identity")
                .send()
                .await
                .map_err(|e| api_err(ApiError::Upstream(format!("拉取文件失败：{e}"))))?;
            if !resp.status().is_success() {
                return Err(api_err(ApiError::Upstream(format!(
                    "上游拒绝访问（HTTP {}）",
                    resp.status().as_u16()
                ))));
            }
            let bytes = resp
                .bytes()
                .await
                .map_err(|e| api_err(ApiError::Upstream(format!("读取文件失败：{e}"))))?;
            if bytes.len() as u64 > cfg.doc.max_bytes {
                return Err(api_err(ApiError::TooLarge));
            }
            let text = extract::extract_text(&ext, &bytes).map_err(|e| api_err(ApiError::Internal(e)))?;
            Ok(text)
        }
        other => Err(json!({ "code": -32602, "message": format!("未知工具：{other}") })),
    }?;

    Ok(json!({
        "content": [{ "type": "text", "text": text }],
        "isError": false
    }))
}

fn arg_str(args: &Value, key: &str) -> Result<String, Value> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| json!({ "code": -32602, "message": format!("缺少参数：{key}") }))
}

fn api_err(e: ApiError) -> Value {
    json!({ "code": -32000, "message": e.to_string() })
}

async fn head_size(prep: &doc::Prepared) -> Option<u64> {
    // HEAD 拿不到（或为 0）时降级 Range 探测，与 /api/doc/meta 的行为一致
    let head = prep
        .client
        .head(prep.target.clone())
        .header(reqwest::header::ACCEPT_ENCODING, "identity")
        .send()
        .await
        .ok()?;
    let head_len = head.content_length().filter(|s| *s > 0);
    if head_len.is_some() {
        return head_len;
    }
    let probe = prep
        .client
        .get(prep.target.clone())
        .header(reqwest::header::ACCEPT_ENCODING, "identity")
        .header(reqwest::header::RANGE, "bytes=0-0")
        .send()
        .await
        .ok()?;
    let probe_status = probe.status();
    if probe_status == reqwest::StatusCode::PARTIAL_CONTENT {
        // 206：content_length 是分片长度（1），总长只能从 Content-Range 取
        return size_from_content_range(probe.headers());
    }
    probe
        .content_length()
        .filter(|s| *s > 0)
}

/// 解析 `Content-Range: bytes 0-0/12345`
fn size_from_content_range(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    let raw = headers.get(reqwest::header::CONTENT_RANGE)?.to_str().ok()?;
    let total = raw.rsplit('/').next()?.trim();
    total.parse::<u64>().ok()
}
