use std::{
    env, fs,
    net::{IpAddr, SocketAddr},
    path::PathBuf,
};

use anyhow::{Context, Result};

#[derive(Clone)]
pub struct Config {
    pub addr: SocketAddr,
    pub base_url: String,
    pub username: String,
    pub password: String,
    pub skip_tls_verify: bool,
    pub ca_cert: Option<PathBuf>,
    pub default_ttl: u64,
    pub default_comment: Option<String>,
    pub domain_filter: Vec<String>,
    pub exclude_domains: Vec<String>,
    pub regex_include: Option<String>,
    pub regex_exclude: Option<String>,
    pub health_addr: SocketAddr,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let host = value("SERVER_HOST", "localhost");
        let port = value("SERVER_PORT", "8888")
            .parse::<u16>()
            .context("SERVER_PORT")?;
        let base = required("MIKROTIK_BASEURL")?;
        let parsed = url::Url::parse(&base).context("MIKROTIK_BASEURL")?;
        if !matches!(parsed.scheme(), "http" | "https")
            || parsed.host().is_none()
            || parsed.username() != ""
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
            || parsed.path() != "/"
        {
            anyhow::bail!(
                "MIKROTIK_BASEURL must be an HTTP(S) origin without credentials, path, query, or fragment"
            );
        }
        let base_url = format!(
            "{}/rest/ip/dns/static",
            parsed.as_str().trim_end_matches('/')
        );
        Ok(Self {
            addr: resolve_addr(&host, port).context("server address")?,
            base_url,
            username: required("MIKROTIK_USERNAME")?,
            password: required("MIKROTIK_PASSWORD")?,
            skip_tls_verify: value("MIKROTIK_SKIP_TLS_VERIFY", "false")
                .parse()
                .context("MIKROTIK_SKIP_TLS_VERIFY")?,
            ca_cert: env::var("MIKROTIK_CA_CERT").ok().map(PathBuf::from),
            default_ttl: value("MIKROTIK_DEFAULT_TTL", "3600")
                .parse()
                .context("MIKROTIK_DEFAULT_TTL")?,
            default_comment: env::var("MIKROTIK_DEFAULT_COMMENT").ok(),
            domain_filter: csv("DOMAIN_FILTER"),
            exclude_domains: csv("EXCLUDE_DOMAIN_FILTER"),
            regex_include: optional("REGEXP_DOMAIN_FILTER"),
            regex_exclude: optional("REGEXP_DOMAIN_FILTER_EXCLUSION"),
            health_addr: resolve_addr(
                &value("HEALTH_HOST", "0.0.0.0"),
                value("HEALTH_PORT", "8080")
                    .parse()
                    .context("HEALTH_PORT")?,
            )?,
        })
    }

    pub fn test() -> Self {
        Self {
            addr: SocketAddr::from(([127, 0, 0, 1], 0)),
            base_url: "http://127.0.0.1/rest/ip/dns/static".into(),
            username: "user".into(),
            password: "pass".into(),
            skip_tls_verify: true,
            ca_cert: None,
            default_ttl: 3600,
            default_comment: None,
            domain_filter: Vec::new(),
            exclude_domains: Vec::new(),
            regex_include: None,
            regex_exclude: None,
            health_addr: SocketAddr::from(([127, 0, 0, 1], 0)),
        }
    }
}

fn value(name: &str, default: &str) -> String {
    env::var(name).unwrap_or_else(|_| default.into())
}
fn optional(name: &str) -> Option<String> {
    env::var(name).ok().filter(|v| !v.is_empty())
}
fn required(name: &str) -> Result<String> {
    env::var(name).with_context(|| format!("{name} is required"))
}
fn csv(name: &str) -> Vec<String> {
    value(name, "")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

fn resolve_addr(host: &str, port: u16) -> Result<SocketAddr> {
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Ok(SocketAddr::new(ip, port));
    }
    if host == "localhost" {
        return Ok(SocketAddr::from(([127, 0, 0, 1], port)));
    }
    format!("{host}:{port}")
        .parse()
        .context("host must be an IP address or localhost")
}

pub fn ca_bytes(path: &Option<PathBuf>) -> Result<Option<Vec<u8>>> {
    path.as_ref()
        .map(fs::read)
        .transpose()
        .context("read CA certificate")
}
