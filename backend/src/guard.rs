use ipnet::IpNet;
use reqwest::Url;
use std::net::{IpAddr, SocketAddr};
use thiserror::Error;

use crate::config::DocConfig;

/// 内网 / 保留 / 组播网段：命中即拒绝，防止 DNS 指向内网做探测。
const DENY_NETS: &[&str] = &[
    "0.0.0.0/8",
    "10.0.0.0/8",
    "100.64.0.0/10",
    "127.0.0.0/8",
    "169.254.0.0/16",
    "172.16.0.0/12",
    "192.168.0.0/16",
    "224.0.0.0/4",
    "240.0.0.0/4",
    "::1/128",
    "fc00::/7",
    "fe80::/10",
];

#[derive(Debug, Error)]
pub enum GuardError {
    #[error("url 不合法")]
    BadUrl,
    #[error("不允许的协议：{0}")]
    BadScheme(String),
    #[error("不允许的端口：{0}")]
    BadPort(u16),
    #[error("域名不在白名单：{0}")]
    HostNotAllowed(String),
    #[error("路径不在白名单：{0}")]
    PathNotAllowed(String),
    #[error("目标解析到内网/保留地址：{0}")]
    InternalAddress(String),
    #[error("目标域名解析失败：{0}")]
    ResolveFailed(String),
}

pub struct UrlGuard {
    allow_scheme: Vec<String>,
    allow_hosts: Vec<(String, Vec<String>)>,
    allow_ports: Vec<u16>,
    deny_nets: Vec<IpNet>,
}

impl UrlGuard {
    pub fn new(
        allow_scheme: Vec<String>,
        allow_hosts: Vec<(String, Vec<String>)>,
        allow_ports: Vec<u16>,
    ) -> Self {
        let deny_nets = DENY_NETS
            .iter()
            .filter_map(|c| c.parse::<IpNet>().ok())
            .collect::<Vec<_>>();
        Self {
            allow_scheme,
            allow_hosts,
            allow_ports,
            deny_nets,
        }
    }

    pub fn from_config(doc: &DocConfig) -> Self {
        let hosts = doc
            .allow_hosts
            .iter()
            .map(|h| (h.host.to_lowercase(), h.paths.clone()))
            .collect::<Vec<_>>();
        Self::new(doc.allow_scheme.clone(), hosts, doc.allow_ports.clone())
    }

    /// 第一层：scheme + host + port + path 白名单
    pub fn check_static(&self, raw: &str) -> Result<Url, GuardError> {
        let u = Url::parse(raw.trim()).map_err(|_| GuardError::BadUrl)?;

        if !u.username().is_empty() || u.password().is_some() {
            // 带 userinfo 的 URL 容易被用来欺骗解析器，直接拒绝
            return Err(GuardError::BadUrl);
        }

        let scheme = u.scheme().to_lowercase();
        if !self.allow_scheme.iter().any(|s| s.eq_ignore_ascii_case(&scheme)) {
            return Err(GuardError::BadScheme(scheme));
        }

        let host = u.host_str().ok_or(GuardError::BadUrl)?.to_lowercase();
        let port = u.port_or_known_default().ok_or(GuardError::BadUrl)?;
        if !self.allow_ports.is_empty() && !self.allow_ports.contains(&port) {
            return Err(GuardError::BadPort(port));
        }

        let patterns = self
            .allow_hosts
            .iter()
            .find(|(h, _)| h.eq_ignore_ascii_case(&host))
            .map(|(_, p)| p)
            .ok_or_else(|| GuardError::HostNotAllowed(host.clone()))?;

        let path = u.path();
        if !patterns.iter().any(|pat| match_path(pat, path)) {
            return Err(GuardError::PathNotAllowed(path.to_string()));
        }

        Ok(u)
    }

    /// 第二层：解析 DNS，逐个 IP 校验，返回一个可直接 pin 的地址。
    /// 调用方必须用 `ClientBuilder::resolve(host, addr)` 固定该地址，
    /// 否则 DNS 重绑定（TOCTOU）仍有机会把连接指回内网。
    pub async fn resolve_pinned(&self, u: &Url) -> Result<SocketAddr, GuardError> {
        let host = u.host_str().ok_or(GuardError::BadUrl)?.to_string();
        let port = u.port_or_known_default().ok_or(GuardError::BadUrl)?;

        let resolved = tokio::net::lookup_host((host.as_str(), port))
            .await
            .map_err(|_| GuardError::ResolveFailed(host.clone()))?;

        let mut denied = 0usize;
        for addr in resolved {
            if self.is_denied(addr.ip()) {
                denied += 1;
                continue;
            }
            return Ok(addr);
        }

        if denied > 0 {
            Err(GuardError::InternalAddress(host))
        } else {
            Err(GuardError::ResolveFailed(host))
        }
    }

    fn is_denied(&self, ip: IpAddr) -> bool {
        self.deny_nets.iter().any(|net| net.contains(&ip))
    }
}

fn match_path(pattern: &str, path: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => path.starts_with(prefix),
        None => pattern == path,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guard() -> UrlGuard {
        UrlGuard::new(
            vec!["https".to_string()],
            vec![
                ("cdn.example.com".to_string(), vec!["/docs/*".to_string()]),
                ("files.example.com".to_string(), vec!["/*".to_string()]),
            ],
            vec![443],
        )
    }

    #[test]
    fn accepts_whitelisted_inputs() {
        let g = guard();
        for u in [
            "https://cdn.example.com/docs/a.docx",
            "https://cdn.example.com/docs/sub/b/x.pptx",
            "https://files.example.com/a.xlsx",
            "https://files.example.com/a/b/c",
            "https://files.example.com:443/a.docx",
            "HTTPS://CDN.Example.COM/docs/a.docx",
            "https://cdn.example.com/docs/a.docx?token=abc",
            "https://cdn.example.com/docs/a.docx#frag",
            "  https://cdn.example.com/docs/a.docx  ",
            "https://cdn.example.com/docs/",
        ] {
            g.check_static(u).unwrap_or_else(|e| panic!("应放行: {u} ({e})"));
        }
    }

    #[test]
    fn rejects_bad_scheme() {
        let g = guard();
        for u in [
            "http://cdn.example.com/docs/a.docx",
            "ftp://cdn.example.com/docs/a.docx",
            "file:///etc/passwd",
            "gopher://files.example.com/a",
            "ws://files.example.com/a",
            "data:text/plain,x",
        ] {
            assert!(
                matches!(g.check_static(u), Err(GuardError::BadScheme(_))),
                "应拒绝协议: {u}"
            );
        }
    }

    #[test]
    fn rejects_bad_port() {
        let g = guard();
        for (u, port) in [
            ("https://cdn.example.com:8443/docs/a.docx", 8443),
            ("https://cdn.example.com:80/docs/a.docx", 80),
            ("https://files.example.com:22/a", 22),
            ("https://files.example.com:444/a", 444),
        ] {
            match g.check_static(u) {
                Err(GuardError::BadPort(p)) => assert_eq!(p, port, "{u}"),
                other => panic!("应报 BadPort({port}): {u}, 实际 {other:?}"),
            }
        }
    }

    #[test]
    fn rejects_non_whitelisted_host() {
        let g = guard();
        for u in [
            "https://evil.example.com/docs/a.docx",
            "https://cdn.example.com.evil.com/docs/a.docx",
            "https://notcdn.example.com/docs/a.docx",
            "https://cdn.example.com@evil.com/a",
            "https://127.0.0.1/a.docx",
        ] {
            let err = g.check_static(u).unwrap_err();
            assert!(
                matches!(err, GuardError::HostNotAllowed(_))
                    || matches!(err, GuardError::BadUrl),
                "应拒绝域名/userinfo: {u}, 实际 {err:?}"
            );
        }
    }

    #[test]
    fn rejects_credentials_in_url() {
        let g = guard();
        for u in [
            "https://user:pass@files.example.com/a.docx",
            "https://user@files.example.com/a.docx",
            "https://:secret@files.example.com/a.docx",
        ] {
            assert!(matches!(g.check_static(u), Err(GuardError::BadUrl)), "{u}");
        }
    }

    #[test]
    fn rejects_path_outside_whitelist() {
        let g = guard();
        for u in [
            "https://cdn.example.com/private/a.docx",
            "https://cdn.example.com/docsx/a.docx",
            "https://cdn.example.com/docsa",
            "https://cdn.example.com/docs",
        ] {
            assert!(
                matches!(g.check_static(u), Err(GuardError::PathNotAllowed(_))),
                "应拒绝路径: {u}"
            );
        }
    }

    #[test]
    fn rejects_malformed_url() {
        let g = guard();
        for u in ["", "   ", "not a url", "://nohost", "https://"] {
            assert!(matches!(g.check_static(u), Err(GuardError::BadUrl)), "{u:?}");
        }
        // 三个斜杠：url crate 解析出空 host，应落到 HostNotAllowed 而不是 panic
        assert!(matches!(
            g.check_static("https:///docs/a.docx"),
            Err(GuardError::HostNotAllowed(_))
        ));
    }

    #[test]
    fn empty_port_allowlist_allows_any_port() {
        let g = UrlGuard::new(
            vec!["https".to_string()],
            vec![("files.example.com".to_string(), vec!["/*".to_string()])],
            vec![],
        );
        assert!(g.check_static("https://files.example.com:8080/a.docx").is_ok());
        assert!(g.check_static("https://files.example.com/a.docx").is_ok());
    }

    #[test]
    fn match_path_exact_and_prefix() {
        assert!(match_path("/docs/*", "/docs/"));
        assert!(match_path("/docs/*", "/docs/a/b.docx"));
        assert!(!match_path("/docs/*", "/docsx/a"));
        assert!(!match_path("/docs/*", "/docs"));
        assert!(match_path("/*", "/anything/at/all"));
        assert!(match_path("/*", "/"));
        assert!(match_path("/", "/"));
        assert!(!match_path("/", "/a"));
        assert!(match_path("/api/*", "/api/x"));
        assert!(!match_path("/api/*", "/api"));
        assert!(!match_path("/api/*", "/apix"));
    }

    #[test]
    fn denies_private_and_reserved_ranges() {
        let g = guard();
        for ip in [
            "0.0.0.0",
            "0.1.2.3",
            "10.0.0.1",
            "10.255.255.255",
            "100.64.0.1",
            "100.127.255.255",
            "127.0.0.1",
            "127.255.255.255",
            "169.254.0.1",
            "169.254.169.254",
            "172.16.0.1",
            "172.31.255.255",
            "192.168.0.1",
            "192.168.255.255",
            "224.0.0.1",
            "239.255.255.255",
            "240.0.0.1",
            "255.255.255.255",
            "::1",
            "fe80::1",
            "fd00::1",
            "fc00::1",
        ] {
            assert!(g.is_denied(ip.parse().unwrap()), "{ip} 应被拒绝");
        }
        for ip in [
            "1.1.1.1",
            "8.8.8.8",
            "11.0.0.1",
            "172.32.0.1",
            "192.169.0.1",
            "100.128.0.1",
            "169.253.255.255",
            "203.0.113.9",
            "8.8.4.4",
            "2606:4700::4700",
        ] {
            assert!(!g.is_denied(ip.parse().unwrap()), "{ip} 不应被拒绝");
        }
    }

    #[tokio::test]
    async fn resolve_pinned_rejects_loopback_literal() {
        let g = guard();
        for h in ["127.0.0.1", "127.0.0.2", "[::1]", "169.254.169.254"] {
            let u = Url::parse(&format!("https://{h}/a.docx")).unwrap();
            assert!(
                matches!(g.resolve_pinned(&u).await, Err(GuardError::InternalAddress(_))),
                "{h} 应命中内网拒绝"
            );
        }
    }

    #[tokio::test]
    async fn resolve_pinned_reports_unresolvable_host() {
        let g = guard();
        let u = Url::parse("https://nonexistent-host.invalid/a.docx").unwrap();
        assert!(matches!(
            g.resolve_pinned(&u).await,
            Err(GuardError::ResolveFailed(_))
        ));
    }
}
