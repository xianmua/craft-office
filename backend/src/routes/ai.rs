//! AI 对话代理：API key 只存在服务端，前端永远接触不到。
//! OpenAI 兼容协议（/chat/completions），stream 由请求体决定。
//! 双引擎：deepseek（直连，需 key）/ dsh（dsh-apis-plugin，本机 3088，
//! OpenAI 兼容且无鉴权，key 可选；body 带 engine 字段分流）。

use std::sync::RwLock;
use std::time::Duration;

use actix_web::{get, post, put, web, HttpResponse};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};

use crate::error::ApiError;

#[derive(Clone)]
struct EngineConfig {
    key: String,
    base_url: String,
    model: String,
}

#[derive(Clone, Default)]
struct AiConfig {
    deepseek: Option<EngineConfig>,
    dsh: Option<EngineConfig>,
}

/// 进程内配置缓存（RwLock 支持设置面板热更）；首次访问从环境变量加载。
/// 更新配置 = 重写缓存 + upsert 到 .env（重启后由 dotenvy 读回）。
static CFG: RwLock<Option<AiConfig>> = RwLock::new(None);

/// 从环境变量读引擎配置；key_required=false 的引擎（如 dsh-apis-plugin 无鉴权）key 可缺省
fn engine_from_env(prefix: &str, default_base: &str, default_model: &str, key_required: bool) -> Option<EngineConfig> {
    let key = std::env::var(format!("{prefix}_API_KEY")).unwrap_or_default().trim().to_string();
    if key_required && key.is_empty() {
        return None;
    }
    Some(EngineConfig {
        base_url: std::env::var(format!("{prefix}_BASE_URL"))
            .unwrap_or_else(|_| default_base.to_string())
            .trim_end_matches('/')
            .to_string(),
        model: std::env::var(format!("{prefix}_MODEL")).unwrap_or_else(|_| default_model.to_string()),
        key,
    })
}

fn config() -> Result<std::sync::RwLockReadGuard<'static, Option<AiConfig>>, ApiError> {
    if CFG.read().unwrap_or_else(|e| e.into_inner()).is_none() {
        *CFG.write().unwrap_or_else(|e| e.into_inner()) = Some(AiConfig {
            deepseek: engine_from_env("DEEPSEEK", "https://api.deepseek.com", "deepseek-chat", true),
            dsh: engine_from_env("DSH", "http://127.0.0.1:3088/v1", "id456/glm-5.3-flash", false),
        });
    }
    Ok(CFG.read().unwrap_or_else(|e| e.into_inner()))
}

fn engine_cfg<'a>(
    cfg: &'a AiConfig,
    engine: Option<&str>,
) -> Result<(&'a EngineConfig, &'static str), ApiError> {
    match engine.unwrap_or("deepseek") {
        "deepseek" => cfg
            .deepseek
            .as_ref()
            .map(|c| (c, "deepseek"))
            .ok_or_else(|| {
                ApiError::NotConfigured(
                    "未配置 DEEPSEEK_API_KEY：复制 backend/.env.example 为 .env 并填入后重启".to_string(),
                )
            }),
        "dsh" => cfg.dsh.as_ref().map(|c| (c, "dsh")).ok_or_else(|| {
            ApiError::NotConfigured(
                "DSH 引擎不可用：确认 dsh-apis-plugin 已启用（默认 http://127.0.0.1:3088）".to_string(),
            )
        }),
        other => Err(ApiError::Internal(format!(
            "未知 engine：{other}（可选 deepseek / dsh）"
        ))),
    }
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
    /// 引擎分流：deepseek（默认）/ dsh
    #[serde(default)]
    pub engine: Option<String>,
}

fn default_stream() -> bool {
    true
}

/// 对话代理：body 原样转发到所选引擎（补全 model），响应按是否流式透传。
#[post("/api/ai/chat")]
pub async fn ai_chat(body: web::Json<ChatBody>) -> Result<HttpResponse, ApiError> {
    let cfg = config()?;
    let fallback = AiConfig::default();
    let cfg_ref = cfg.as_ref().unwrap_or(&fallback);
    let (engine, name) = engine_cfg(cfg_ref, body.engine.as_deref())?;

    if body.messages.is_empty() {
        return Err(ApiError::Internal("messages 不能为空".to_string()));
    }

    let payload = serde_json::json!({
        "model": engine.model,
        "messages": body.messages,
        "stream": body.stream,
        "temperature": body.temperature,
    });

    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| ApiError::Internal(format!("构建 HTTP 客户端失败：{e}")))?;

    let mut req = client
        .post(format!("{}/chat/completions", engine.base_url))
        .json(&payload);
    if !engine.key.is_empty() {
        req = req.bearer_auth(&engine.key);
    }
    let resp = req
        .send()
        .await
        .map_err(|e| ApiError::Upstream(format!("请求 {name} 失败：{e}")))?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let detail = resp.text().await.unwrap_or_default();
        let brief: String = detail.chars().take(500).collect();
        return Err(ApiError::Upstream(format!(
            "{name} 返回 HTTP {status}：{brief}"
        )));
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
            .map_err(|e| ApiError::Upstream(format!("读取 {name} 响应失败：{e}")))?;
        Ok(HttpResponse::Ok().content_type("application/json").body(bytes))
    }
}

// ===== 设置面板：配置读写（key 只回脱敏掩码）+ 模型列表 =====

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EngineInfo {
    configured: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    api_key_masked: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<String>,
}

fn mask_key(k: &str) -> String {
    if k.len() <= 10 {
        "***".to_string()
    } else {
        format!("{}...{}", &k[..6], &k[k.len() - 4..])
    }
}

fn engine_info(c: Option<&EngineConfig>) -> EngineInfo {
    match c {
        Some(c) => EngineInfo {
            configured: true,
            // 无鉴权引擎（key 为空）不回掩码
            api_key_masked: (!c.key.is_empty()).then(|| mask_key(&c.key)),
            base_url: Some(c.base_url.clone()),
            model: Some(c.model.clone()),
        },
        None => EngineInfo {
            configured: false,
            api_key_masked: None,
            base_url: None,
            model: None,
        },
    }
}

fn prefix_of(engine: &str) -> Result<&'static str, ApiError> {
    match engine {
        "deepseek" => Ok("DEEPSEEK"),
        "dsh" => Ok("DSH"),
        other => Err(ApiError::Internal(format!(
            "未知 engine：{other}（可选 deepseek / dsh）"
        ))),
    }
}

/// upsert .env 中 `KEY=VALUE` 行（不存在则追加）；dotenvy 兼容的无引号写法
fn persist_env(pairs: &[(&str, String)]) -> Result<(), ApiError> {
    let path = std::path::Path::new(".env");
    let content = std::fs::read_to_string(path).unwrap_or_default();
    let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
    for (k, v) in pairs {
        let prefix = format!("{k}=");
        match lines.iter().position(|l| l.trim_start().starts_with(&prefix)) {
            Some(pos) => lines[pos] = format!("{k}={v}"),
            None => lines.push(format!("{k}={v}")),
        }
    }
    std::fs::write(path, lines.join("\n") + "\n")
        .map_err(|e| ApiError::Internal(format!("写入 .env 失败：{e}")))
}

/// 配置读取：两个引擎的脱敏视图（key 永不回传明文）
#[get("/api/ai/config")]
pub async fn ai_config_get() -> Result<HttpResponse, ApiError> {
    let cfg = config()?;
    let fallback = AiConfig::default();
    let cfg_ref = cfg.as_ref().unwrap_or(&fallback);
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "deepseek": engine_info(cfg_ref.deepseek.as_ref()),
        "dsh": engine_info(cfg_ref.dsh.as_ref()),
    })))
}

#[derive(Deserialize)]
pub struct ConfigUpdate {
    pub engine: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
}

/// 配置更新：合并进当前引擎配置（key 留空 = 不变），即时生效并持久化 .env
#[put("/api/ai/config")]
pub async fn ai_config_put(body: web::Json<ConfigUpdate>) -> Result<HttpResponse, ApiError> {
    let prefix = prefix_of(&body.engine)?;
    let (defaults_base, defaults_model, key_required) = match prefix {
        "DEEPSEEK" => ("https://api.deepseek.com", "deepseek-chat", true),
        _ => ("http://127.0.0.1:3088/v1", "id456/glm-5.3-flash", false),
    };

    let cfg = config()?;
    let fallback = AiConfig::default();
    let cfg_ref = cfg.as_ref().unwrap_or(&fallback);
    let cur_cfg = match prefix {
        "DEEPSEEK" => cfg_ref.deepseek.clone(),
        _ => cfg_ref.dsh.clone(),
    };
    drop(cfg);

    let key = match body.api_key.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(k) => k.to_string(),
        None => match cur_cfg.as_ref().map(|c| c.key.clone()) {
            Some(k) => k,
            None if key_required => {
                return Err(ApiError::Internal(format!("{prefix}_API_KEY 尚未配置，请填写 API Key")));
            }
            None => String::new(), // 无鉴权引擎（dsh-apis-plugin）允许空 key
        },
    };
    let base_url = body
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.trim_end_matches('/').to_string())
        .or_else(|| cur_cfg.as_ref().map(|c| c.base_url.clone()))
        .unwrap_or_else(|| defaults_base.to_string());
    let model = body
        .model
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .or_else(|| cur_cfg.as_ref().map(|c| c.model.clone()))
        .unwrap_or_else(|| defaults_model.to_string());

    persist_env(&[
        (&format!("{prefix}_API_KEY"), key.clone()),
        (&format!("{prefix}_BASE_URL"), base_url.clone()),
        (&format!("{prefix}_MODEL"), model.clone()),
    ])?;

    let updated = EngineConfig { key, base_url, model };
    {
        let mut w = CFG.write().unwrap_or_else(|e| e.into_inner());
        let cfg = w.get_or_insert_with(Default::default);
        match prefix {
            "DEEPSEEK" => cfg.deepseek = Some(updated.clone()),
            _ => cfg.dsh = Some(updated.clone()),
        }
    }

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "ok": true, "apiKeyMasked": mask_key(&updated.key), "baseUrl": updated.base_url, "model": updated.model
    })))
}

/// 可用模型列表：用当前配置调 {base}/models（OpenAI 兼容），供设置面板下拉
#[get("/api/ai/models")]
pub async fn ai_models(q: web::Query<std::collections::HashMap<String, String>>) -> Result<HttpResponse, ApiError> {
    let engine = q.get("engine").map(String::as_str).unwrap_or("deepseek");
    let cfg = config()?;
    let fallback = AiConfig::default();
    let cfg_ref = cfg.as_ref().unwrap_or(&fallback);
    let (ec, name) = engine_cfg(cfg_ref, Some(engine))?;
    let ec = ec.clone();
    drop(cfg);

    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| ApiError::Internal(format!("构建 HTTP 客户端失败：{e}")))?;

    let mut req = client
        .get(format!("{}/models", ec.base_url));
    if !ec.key.is_empty() {
        req = req.bearer_auth(&ec.key);
    }
    let resp = req
        .send()
        .await
        .map_err(|e| ApiError::Upstream(format!("请求 {name} 模型列表失败：{e}")))?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let detail = resp.text().await.unwrap_or_default();
        let brief: String = detail.chars().take(300).collect();
        return Err(ApiError::Upstream(format!(
            "{name} 模型列表返回 HTTP {status}：{brief}"
        )));
    }

    let body: serde_json::Value = resp.json().await.map_err(|e| ApiError::Upstream(format!("解析模型列表失败：{e}")))?;
    let models: Vec<String> = body
        .get("data")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m.get("id").and_then(|v| v.as_str()))
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default();
    Ok(HttpResponse::Ok().json(serde_json::json!({ "models": models })))
}
