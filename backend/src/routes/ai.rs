//! AI 对话代理：API key 只存在服务端，前端永远接触不到。
//! OpenAI 兼容协议（/chat/completions），stream 由请求体决定。
//! 引擎注册表：内置 deepseek + plugins/*/manifest.json（含 engine 节）声明的插件引擎，
//! body 带 engine 字段（引擎 id）分流；新增引擎只需加插件目录，无需改后端代码。

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use actix_web::{get, post, put, web, HttpResponse};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};

use crate::config::AppConfig;
use crate::error::ApiError;

#[derive(Clone)]
struct EngineConfig {
    key: String,
    base_url: String,
    model: String,
}

/// 插件统一入口声明（plugins/<dir>/manifest.json）：身份 + 类型化配置节
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PluginManifest {
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
    /// AI 引擎插件必有此节；其他类型插件缺省
    #[serde(default)]
    engine: Option<EngineSection>,
}

/// manifest.engine 节：引擎行为配置（身份字段 id/label/desc 复用 manifest 顶层）
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EngineSection {
    #[serde(default = "default_protocol")]
    protocol: String,
    #[serde(default)]
    default_base_url: String,
    #[serde(default)]
    default_model: String,
    #[serde(default)]
    key_required: bool,
    /// 配置持久化到 .env 的键前缀；缺省 = id 大写
    #[serde(default)]
    env_prefix: String,
}

/// 引擎注册表条目（内置 + 由 manifest.json 的 engine 节构造）
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineMeta {
    pub id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub desc: String,
    /// 协议族；当前仅支持 openai 兼容
    #[serde(default = "default_protocol")]
    pub protocol: String,
    #[serde(default)]
    pub default_base_url: String,
    #[serde(default)]
    pub default_model: String,
    #[serde(default)]
    pub key_required: bool,
    /// 配置持久化到 .env 的键前缀；缺省 = id 大写
    #[serde(default)]
    pub env_prefix: String,
}

fn default_protocol() -> String {
    "openai".to_string()
}

impl EngineMeta {
    fn prefix(&self) -> String {
        if self.env_prefix.is_empty() {
            self.id.to_uppercase()
        } else {
            self.env_prefix.clone()
        }
    }
}

/// 内置引擎（不依赖插件目录，保证开箱可用）
fn builtin_engines() -> Vec<EngineMeta> {
    vec![EngineMeta {
        id: "deepseek".to_string(),
        label: "LLM AI".to_string(),
        desc: "DeepSeek 网关（OpenAI 兼容）".to_string(),
        protocol: "openai".to_string(),
        default_base_url: "http://ai-gateway.linkskycloud.com/v1".to_string(),
        default_model: "deepseek-v4.1-flash".to_string(),
        key_required: true,
        env_prefix: "DEEPSEEK".to_string(),
    }]
}

/// 扫描 <dir>/*/manifest.json：含 engine 节的注册为插件引擎（市场里启停联动，见 registry）
fn plugin_engines(dev_dir: &str) -> Vec<EngineMeta> {
    let mut list = Vec::new();
    let Ok(entries) = std::fs::read_dir(Path::new(dev_dir)) else {
        return list;
    };
    for item in entries.flatten() {
        let f = item.path().join("manifest.json");
        let Ok(raw) = std::fs::read_to_string(&f) else { continue };
        match serde_json::from_str::<PluginManifest>(&raw) {
            Ok(mf) => {
                if let Some(eng) = mf.engine {
                    list.push(EngineMeta {
                        id: mf.id,
                        label: mf.name,
                        desc: mf.description,
                        protocol: eng.protocol,
                        default_base_url: eng.default_base_url,
                        default_model: eng.default_model,
                        key_required: eng.key_required,
                        env_prefix: eng.env_prefix,
                    });
                }
            }
            Err(e) => eprintln!("[ai] 引擎插件 {} 解析失败：{e}", f.display()),
        }
    }
    list
}

/// 引擎注册表：内置在前，插件引擎同 id 不覆盖内置；插件引擎受市场启停约束
fn registry(cfg: &AppConfig) -> Vec<EngineMeta> {
    let mut list = builtin_engines();
    let disabled = crate::routes::plugins::disabled_set(cfg);
    for m in plugin_engines(&cfg.plugins.dev_dir) {
        if !list.iter().any(|e| e.id == m.id) && !disabled.contains(&m.id) {
            list.push(m);
        }
    }
    list
}

fn find_meta(cfg: &AppConfig, id: &str) -> Option<EngineMeta> {
    registry(cfg).into_iter().find(|m| m.id == id)
}

/// 进程内配置缓存（RwLock 支持设置面板热更）；首次访问从环境变量加载全部已注册引擎。
/// 更新配置 = 重写缓存 + upsert 到 .env（重启后由 dotenvy 读回）。
static CFG: RwLock<Option<HashMap<String, EngineConfig>>> = RwLock::new(None);

/// 从环境变量读引擎配置；key_required=false 的引擎（如 dsh-apis-plugin 无鉴权）key 可缺省
fn engine_from_env(meta: &EngineMeta) -> Option<EngineConfig> {
    let key = std::env::var(format!("{}_API_KEY", meta.prefix()))
        .unwrap_or_default()
        .trim()
        .to_string();
    if meta.key_required && key.is_empty() {
        return None;
    }
    Some(EngineConfig {
        base_url: std::env::var(format!("{}_BASE_URL", meta.prefix()))
            .unwrap_or_else(|_| meta.default_base_url.clone())
            .trim_end_matches('/')
            .to_string(),
        model: std::env::var(format!("{}_MODEL", meta.prefix()))
            .unwrap_or_else(|_| meta.default_model.clone()),
        key,
    })
}

fn config(cfg: &AppConfig) -> Result<HashMap<String, EngineConfig>, ApiError> {
    if CFG.read().unwrap_or_else(|e| e.into_inner()).is_none() {
        let map: HashMap<String, EngineConfig> = registry(cfg)
            .iter()
            .filter_map(|m| engine_from_env(m).map(|c| (m.id.clone(), c)))
            .collect();
        *CFG.write().unwrap_or_else(|e| e.into_inner()) = Some(map);
    }
    Ok(CFG.read().unwrap_or_else(|e| e.into_inner()).clone().unwrap_or_default())
}

fn engine_cfg(
    cfg: &AppConfig,
    map: &HashMap<String, EngineConfig>,
    engine: Option<&str>,
) -> Result<(EngineConfig, EngineMeta), ApiError> {
    let id = engine.unwrap_or("deepseek");
    let meta = find_meta(cfg, id).ok_or_else(|| {
        ApiError::Internal(format!("未知引擎：{id}（内置 deepseek 或 plugins 目录 manifest.json engine 节声明）"))
    })?;
    let ec = map.get(id).cloned().ok_or_else(|| {
        ApiError::NotConfigured(format!(
            "引擎 {id} 不可用：请在设置面板填写配置（或 backend/.env 配 {}_*）",
            meta.prefix()
        ))
    })?;
    Ok((ec, meta))
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
    /// 引擎分流：内置 deepseek（默认）/ 插件引擎 id（见 plugins/*/manifest.json engine 节）
    #[serde(default)]
    pub engine: Option<String>,
}

fn default_stream() -> bool {
    true
}

/// 对话代理：body 原样转发到所选引擎（补全 model），响应按是否流式透传。
#[post("/api/ai/chat")]
pub async fn ai_chat(body: web::Json<ChatBody>, data: web::Data<Arc<AppConfig>>) -> Result<HttpResponse, ApiError> {
    let cfg = config(&data)?;
    let (engine, meta) = engine_cfg(data.get_ref(), &cfg, body.engine.as_deref())?;
    let name = if meta.label.is_empty() { meta.id.clone() } else { meta.label };

    if body.messages.is_empty() {
        return Err(ApiError::Internal("messages 不能为空".to_string()));
    }

    if meta.protocol != "openai" {
        return Err(ApiError::Internal(format!(
            "引擎 {id} 协议 {proto} 暂不支持（当前仅 openai 兼容）",
            id = meta.id,
            proto = meta.protocol
        )));
    }

    // model 为空 = 不传该字段，由引擎服务端用自己的默认模型（不同部署的模型名前缀各异，留空最通用）
    let mut payload = serde_json::json!({
        "messages": body.messages,
        "stream": body.stream,
        "temperature": body.temperature,
    });
    if !engine.model.is_empty() {
        payload["model"] = serde_json::Value::String(engine.model.clone());
    }

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
    id: String,
    label: String,
    desc: String,
    key_required: bool,
    default_base_url: String,
    default_model: String,
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

fn engine_info(meta: &EngineMeta, c: Option<&EngineConfig>) -> EngineInfo {
    let (configured, api_key_masked, base_url, model) = match c {
        Some(c) => (
            true,
            (!c.key.is_empty()).then(|| mask_key(&c.key)), // 无鉴权引擎不回掩码
            Some(c.base_url.clone()),
            Some(c.model.clone()),
        ),
        None => (false, None, None, None),
    };
    EngineInfo {
        id: meta.id.clone(),
        label: if meta.label.is_empty() { meta.id.clone() } else { meta.label.clone() },
        desc: meta.desc.clone(),
        key_required: meta.key_required,
        default_base_url: meta.default_base_url.clone(),
        default_model: meta.default_model.clone(),
        configured,
        api_key_masked,
        base_url,
        model,
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

/// 配置读取：全部已注册引擎的脱敏视图（key 永不回传明文），数组按注册表顺序
#[get("/api/ai/config")]
pub async fn ai_config_get(data: web::Data<Arc<AppConfig>>) -> Result<HttpResponse, ApiError> {
    let cfg = config(&data)?;
    let list: Vec<EngineInfo> = registry(data.get_ref())
        .iter()
        .map(|m| engine_info(m, cfg.get(&m.id)))
        .collect();
    Ok(HttpResponse::Ok().json(list))
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
pub async fn ai_config_put(
    body: web::Json<ConfigUpdate>,
    data: web::Data<Arc<AppConfig>>,
) -> Result<HttpResponse, ApiError> {
    let meta = find_meta(&data, &body.engine)
        .ok_or_else(|| ApiError::Internal(format!("未知引擎：{}", body.engine)))?;
    let prefix = meta.prefix();

    let cfg = config(&data)?;
    let cur_cfg = cfg.get(&body.engine).cloned();

    let key = match body.api_key.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(k) => k.to_string(),
        None => match cur_cfg.as_ref().map(|c| c.key.clone()) {
            Some(k) => k,
            None if meta.key_required => {
                return Err(ApiError::Internal(format!("{prefix}_API_KEY 尚未配置，请填写 API Key")));
            }
            None => String::new(), // 无鉴权引擎允许空 key
        },
    };
    let base_url = body
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.trim_end_matches('/').to_string())
        .or_else(|| cur_cfg.as_ref().map(|c| c.base_url.clone()))
        .unwrap_or_else(|| meta.default_base_url.clone());
    let model = body
        .model
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .or_else(|| cur_cfg.as_ref().map(|c| c.model.clone()))
        .unwrap_or_else(|| meta.default_model.clone());

    persist_env(&[
        (&format!("{prefix}_API_KEY"), key.clone()),
        (&format!("{prefix}_BASE_URL"), base_url.clone()),
        (&format!("{prefix}_MODEL"), model.clone()),
    ])?;

    let updated = EngineConfig { key, base_url, model };
    CFG.write()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(HashMap::new)
        .insert(body.engine.clone(), updated.clone());

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "ok": true, "apiKeyMasked": mask_key(&updated.key), "baseUrl": updated.base_url, "model": updated.model
    })))
}
