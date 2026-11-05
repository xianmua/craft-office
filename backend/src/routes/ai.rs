//! DeepSeek 对话代理：API key 只存在服务端，前端永远接触不到。
//! OpenAI 兼容协议（DeepSeek 的 /chat/completions），stream 由请求体决定。

use std::time::Duration;

use actix_web::{post, web, HttpResponse};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

use crate::error::ApiError;

struct AiConfig {
    key: String,
    base_url: String,
    model: String,
}

/// 启动后读取一次；未配置 key 时 /api/ai/chat 返回 503 not_configured。
fn config() -> Result<&'static AiConfig, ApiError> {
    static CFG: OnceLock<Option<AiConfig>> = OnceLock::new();
    let cfg = CFG.get_or_init(|| {
        let key = std::env::var("DEEPSEEK_API_KEY").ok()?.trim().to_string();
        if key.is_empty() {
            return None;
        }
        Some(AiConfig {
            key,
            base_url: std::env::var("DEEPSEEK_BASE_URL")
                .unwrap_or_else(|_| "https://api.deepseek.com".to_string())
                .trim_end_matches('/')
                .to_string(),
            model: std::env::var("DEEPSEEK_MODEL").unwrap_or_else(|_| "deepseek-chat".to_string()),
        })
    });
    cfg.as_ref().ok_or_else(|| {
        ApiError::NotConfigured(
            "未配置 DEEPSEEK_API_KEY：复制 backend/.env.example 为 .env 并填入后重启".to_string(),
        )
    })
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct ChatBody {
    pub messages: Vec<ChatMessage>,
    #[serde(default = "default_stream")]
    pub stream: bool,
    #[serde(default)]
    pub temperature: Option<f32>,
}

fn default_stream() -> bool {
    true
}

/// 对话代理：body 原样转发到 DeepSeek（补全 model），响应按是否流式透传。
#[post("/api/ai/chat")]
pub async fn ai_chat(body: web::Json<ChatBody>) -> Result<HttpResponse, ApiError> {
    let cfg = config()?;

    if body.messages.is_empty() {
        return Err(ApiError::Internal("messages 不能为空".to_string()));
    }

    let payload = serde_json::json!({
        "model": cfg.model,
        "messages": body.messages,
        "stream": body.stream,
        "temperature": body.temperature,
    });

    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| ApiError::Internal(format!("构建 HTTP 客户端失败：{e}")))?;

    let resp = client
        .post(format!("{}/chat/completions", cfg.base_url))
        .bearer_auth(&cfg.key)
        .json(&payload)
        .send()
        .await
        .map_err(|e| ApiError::Upstream(format!("请求 DeepSeek 失败：{e}")))?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let detail = resp.text().await.unwrap_or_default();
        let brief: String = detail.chars().take(500).collect();
        return Err(ApiError::Upstream(format!("DeepSeek 返回 HTTP {status}：{brief}")));
    }

    if body.stream {
        let stream = resp
            .bytes_stream()
            .map(|r| r.map_err(|e| std::io::Error::other(e.to_string())));
        Ok(HttpResponse::Ok()
            .content_type("text/event-stream; charset=utf-8")
            .insert_header(("Cache-Control", "no-cache"))
            .streaming(stream))
    } else {
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| ApiError::Upstream(format!("读取 DeepSeek 响应失败：{e}")))?;
        Ok(HttpResponse::Ok().content_type("application/json").body(bytes))
    }
}
