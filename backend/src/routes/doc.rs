use actix_web::{get, web, HttpResponse};
use futures_util::{Stream, StreamExt};
use reqwest::header::{ACCEPT_ENCODING, CONTENT_LENGTH, CONTENT_RANGE, RANGE};
use reqwest::{redirect::Policy, Url};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use crate::config::AppConfig;
use crate::error::ApiError;
use crate::guard::UrlGuard;

#[derive(Debug, Deserialize)]
pub struct DocQuery {
    pub url: String,
    #[serde(default)]
    pub download: bool,
}

#[derive(Debug, Serialize)]
pub struct DocMeta {
    pub ext: String,
    pub mime: String,
    pub size: Option<u64>,
    pub supported: bool,
}

pub(crate) struct Prepared {
    pub client: reqwest::Client,
    pub target: Url,
}

#[derive(Clone, Copy)]
pub(crate) enum Purpose {
    /// 探测元信息：请求极小，用整体超时即可
    Meta,
    /// 流式转发：不能限制整体耗时（大文件 + 慢上游会被误杀），只限制连接与读空闲
    Stream,
}

/// 统一的前置处理：白名单校验 → DNS 解析 → IP 校验 → 用解析结果 pin 住地址。
/// redirect 一律禁止，避免被上游 302 到内网绕过校验。
pub(crate) async fn prepare(
    cfg: &AppConfig,
    guard: &UrlGuard,
    raw: &str,
    purpose: Purpose,
) -> Result<Prepared, ApiError> {
    let target = guard.check_static(raw)?;

    let host = target
        .host_str()
        .ok_or(ApiError::Guard(crate::guard::GuardError::BadUrl))?
        .to_string();
    let addr: SocketAddr = guard.resolve_pinned(&target).await?;

    let builder = reqwest::Client::builder()
        .redirect(Policy::none())
        .user_agent(concat!("artcraft-office/", env!("CARGO_PKG_VERSION")))
        .resolve(&host, addr);

    let builder = match purpose {
        Purpose::Meta => builder.timeout(Duration::from_secs(cfg.doc.timeout_secs)),
        Purpose::Stream => builder
            .connect_timeout(Duration::from_secs(10))
            // read_timeout 是「两次读之间」的空闲超时，而不是整个请求的总时长
            .read_timeout(Duration::from_secs(cfg.doc.timeout_secs)),
    };

    let client = builder
        .build()
        .map_err(|e| ApiError::Internal(format!("构建 HTTP 客户端失败：{e}")))?;

    Ok(Prepared { client, target })
}

/// 元信息：只做 HEAD，不下载正文；HEAD 不被支持时降级为 Range 探测。
#[get("/api/doc/meta")]
pub async fn doc_meta(
    q: web::Query<DocQuery>,
    guard: web::Data<Arc<UrlGuard>>,
    cfg: web::Data<Arc<AppConfig>>,
) -> Result<HttpResponse, ApiError> {
    let prep = prepare(&cfg, &guard, &q.url, Purpose::Meta).await?;

    let ext = detect_ext(prep.target.path());
    let supported = matches!(ext.as_str(), "docx" | "xlsx" | "pptx");

    let head = prep
        .client
        .head(prep.target.clone())
        .header(ACCEPT_ENCODING, "identity")
        .send()
        .await
        .map_err(|e| ApiError::Upstream(format!("探测文件信息失败：{e}")))?;

    let (status, size) = if head.status().is_success() {
        let size = content_length(head.headers());
        (head.status(), size)
    } else {
        let probe = prep
            .client
            .get(prep.target.clone())
            .header(ACCEPT_ENCODING, "identity")
            .header(RANGE, "bytes=0-0")
            .send()
            .await
            .map_err(|e| ApiError::Upstream(format!("探测文件信息失败：{e}")))?;
        let size = content_length(probe.headers()).or_else(|| size_from_content_range(probe.headers()));
        (probe.status(), size)
    };

    if !status.is_success() && status.as_u16() != 206 {
        return Err(ApiError::Upstream(format!(
            "上游拒绝访问（HTTP {}）：可能是签名直链已过期或缺少授权",
            status.as_u16()
        )));
    }

    if let Some(len) = size {
        if len > cfg.doc.max_bytes {
            return Err(ApiError::TooLarge);
        }
    }

    Ok(HttpResponse::Ok().json(DocMeta {
        mime: mime_of(&ext).to_string(),
        ext,
        size,
        supported,
    }))
}

/// 流式代理正文：同源返回，前端不需要关心 CORS / 签名 / Referer。
#[get("/api/doc/proxy")]
pub async fn doc_proxy(
    q: web::Query<DocQuery>,
    guard: web::Data<Arc<UrlGuard>>,
    cfg: web::Data<Arc<AppConfig>>,
) -> Result<HttpResponse, ApiError> {
    let prep = prepare(&cfg, &guard, &q.url, Purpose::Stream).await?;

    let resp = prep
        .client
        .get(prep.target.clone())
        .header(ACCEPT_ENCODING, "identity")
        .send()
        .await
        .map_err(|e| ApiError::Upstream(format!("拉取文件失败：{e}")))?;

    if !resp.status().is_success() {
        return Err(ApiError::Upstream(format!(
            "上游拒绝访问（HTTP {}）：可能是签名直链已过期或缺少授权",
            resp.status().as_u16()
        )));
    }

    if let Some(len) = resp.content_length() {
        if len > cfg.doc.max_bytes {
            return Err(ApiError::TooLarge);
        }
    }

    let ext = detect_ext(prep.target.path());
    let mime = mime_of(&ext);
    let filename = filename_of(&prep.target);

    let disposition = if q.download {
        format!("attachment; filename=\"{filename}\"")
    } else {
        format!("inline; filename=\"{filename}\"")
    };

    let stream = limited_stream(resp.bytes_stream(), cfg.doc.max_bytes);

    Ok(HttpResponse::Ok()
        .insert_header(("Content-Type", mime))
        .insert_header(("Content-Disposition", disposition))
        .insert_header(("Cache-Control", "private, max-age=300"))
        .insert_header(("X-Content-Type-Options", "nosniff"))
        .streaming(stream))
}

/// 流式限流：边转发边计数，超限立即中断（Content-Length 可伪造，不能只靠头部判断）。
fn limited_stream<S>(inner: S, max: u64) -> impl Stream<Item = Result<web::Bytes, std::io::Error>>
where
    S: Stream<Item = Result<web::Bytes, reqwest::Error>>,
{
    futures_util::stream::try_unfold(
        (Box::pin(inner), 0u64),
        move |(mut stream, mut total)| async move {
            match stream.next().await {
                None => Ok(None),
                Some(Ok(chunk)) => {
                    total += chunk.len() as u64;
                    if total > max {
                        return Err(std::io::Error::other(format!(
                            "文件超过大小上限 {max} bytes，已中断传输"
                        )));
                    }
                    Ok(Some((chunk, (stream, total))))
                }
                Some(Err(e)) => Err(std::io::Error::other(e.to_string())),
            }
        },
    )
}

fn content_length(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    headers
        .get(CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok())
}

/// 解析 `Content-Range: bytes 0-0/12345`
fn size_from_content_range(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    let raw = headers.get(CONTENT_RANGE)?.to_str().ok()?;
    let total = raw.rsplit('/').next()?.trim();
    total.parse::<u64>().ok()
}

pub(crate) fn detect_ext(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    match name.rsplit_once('.') {
        Some((_, ext)) if !ext.is_empty() => ext.to_lowercase(),
        _ => String::new(),
    }
}

fn mime_of(ext: &str) -> &'static str {
    match ext {
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "pdf" => "application/pdf",
        "doc" => "application/msword",
        "xls" => "application/vnd.ms-excel",
        "ppt" => "application/vnd.ms-powerpoint",
        "csv" => "text/csv; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// 从 URL 路径取文件名，做最小化清洗（防头注入 / 路径穿越）。
fn filename_of(u: &Url) -> String {
    let raw = u
        .path_segments()
        .and_then(|s| s.last())
        .filter(|s| !s.is_empty())
        .unwrap_or("document");

    let decoded = percent_decode(raw);
    let cleaned: String = decoded
        .chars()
        .filter(|c| !matches!(c, '"' | '\\' | '/' | '\r' | '\n' | '\0'))
        .collect();

    if cleaned.is_empty() {
        "document".to_string()
    } else {
        cleaned
    }
}

fn percent_decode(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = &raw[i + 1..i + 3];
            if let Ok(v) = u8::from_str_radix(hex, 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::{HeaderMap, HeaderValue};

    fn headers_from(pairs: &[(&'static str, &'static str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.insert(*k, HeaderValue::from_static(v));
        }
        h
    }

    #[test]
    fn detects_extension() {
        for (path, want) in [
            ("/a/b/report.DOCX", "docx"),
            ("/a/b/report.docx", "docx"),
            ("/a/b/report", ""),
            ("/a/b/sheet.xlsx", "xlsx"),
            ("/archive.tar.gz", "gz"),
            ("/a.b/c", ""),
            ("/a/b/", ""),
            ("/file", ""),
            ("/docs/.hidden.docx", "docx"),
        ] {
            assert_eq!(detect_ext(path), want, "detect_ext({path})");
        }
    }

    #[test]
    fn maps_mime() {
        assert!(mime_of("docx").contains("wordprocessingml"));
        assert!(mime_of("xlsx").contains("spreadsheetml"));
        assert!(mime_of("pptx").contains("presentationml"));
        assert_eq!(mime_of("pdf"), "application/pdf");
        assert_eq!(mime_of("doc"), "application/msword");
        assert_eq!(mime_of("xls"), "application/vnd.ms-excel");
        assert_eq!(mime_of("ppt"), "application/vnd.ms-powerpoint");
        assert_eq!(mime_of("csv"), "text/csv; charset=utf-8");
        assert_eq!(mime_of("exe"), "application/octet-stream");
        assert_eq!(mime_of("txt"), "application/octet-stream");
        assert_eq!(mime_of(""), "application/octet-stream");
    }

    #[test]
    fn sanitizes_filename() {
        // %E6%8A%98%E5%8F%A0 = 折叠
        let u = Url::parse("https://cdn.example.com/docs/%E6%8A%98%E5%8F%A0.docx").unwrap();
        assert_eq!(filename_of(&u), "折叠.docx");

        let u = Url::parse("https://cdn.example.com/docs/a%22b.docx").unwrap();
        assert!(!filename_of(&u).contains('"'));

        let u = Url::parse("https://cdn.example.com/docs/a%5Cb.docx").unwrap();
        assert!(!filename_of(&u).contains('\\'));

        let u = Url::parse("https://cdn.example.com/").unwrap();
        assert_eq!(filename_of(&u), "document");

        let u = Url::parse("https://cdn.example.com/docs/a%20b.docx").unwrap();
        assert_eq!(filename_of(&u), "a b.docx");

        let u = Url::parse("https://cdn.example.com/docs/line%0Abreak.docx").unwrap();
        assert!(!filename_of(&u).contains('\n'));
    }

    #[test]
    fn decodes_percent_encoding() {
        assert_eq!(percent_decode("%20"), " ");
        assert_eq!(percent_decode("%41"), "A");
        assert_eq!(percent_decode("abc"), "abc");
        // 非法百分号序列保持原样，不 panic
        assert_eq!(percent_decode("%ZZ"), "%ZZ");
        assert_eq!(percent_decode("%2"), "%2");
        // 中文 UTF-8 解码
        assert_eq!(percent_decode("%E6%8A%98%E5%8F%A0"), "折叠");
    }

    #[test]
    fn parses_content_length() {
        assert_eq!(content_length(&headers_from(&[("content-length", "123")])), Some(123));
        assert_eq!(content_length(&headers_from(&[("content-length", " 123 ")])), Some(123));
        assert_eq!(content_length(&headers_from(&[("content-length", "abc")])), None);
        assert_eq!(content_length(&headers_from(&[])), None);
    }

    #[test]
    fn parses_content_range() {
        assert_eq!(
            size_from_content_range(&headers_from(&[("content-range", "bytes 0-0/1024")])),
            Some(1024)
        );
        assert_eq!(
            size_from_content_range(&headers_from(&[("content-range", "bytes 100-200/5000")])),
            Some(5000)
        );
        assert_eq!(
            size_from_content_range(&headers_from(&[("content-range", "bytes abc")])),
            None
        );
        assert_eq!(size_from_content_range(&headers_from(&[])), None);
    }
}
