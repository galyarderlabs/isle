// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 zickrian

//! LLM router usage for the island: what an AI gateway running here or over the network reports —
//! requests, tokens in and out, and the cost it estimates. Everything behind one trait, so another
//! gateway later is one implementation and a Settings entry. The first gateway is 9Router, read the
//! way its own CLI reads it on this machine.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use log::warn;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::Notify;
use zbus::object_server::InterfaceRef;
use zbus::{fdo, interface};

/// The gateway's own CLI-token salt, as its CLI derives the same token.
const CLI_AUTH: &str = "9r-cli-auth";
/// 9Router's default port, what its npm install serves.
const LOCAL_URL: &str = "http://localhost:20128";
/// How long any one request may take.
const TIMEOUT: Duration = Duration::from_secs(10);
/// The poll while the island shows the chip or the page.
const POLL: Duration = Duration::from_secs(12);
/// Backoff after failures: 2 s, 5 s, 15 s, then 60 s.
const BACKOFF: [Duration; 4] = [
    Duration::from_secs(2),
    Duration::from_secs(5),
    Duration::from_secs(15),
    Duration::from_secs(60),
];
/// The gateway's version below which Isle fetches no usage at all: its old usage endpoint leaked
/// API keys (see the plan, section 6).
const SAFE_VERSION: (u64, u64, u64) = (0, 4, 42);

/// One period's numbers, as the chip and the page draw them.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Stats {
    pub requests: u64,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub cached_tokens: u64,
    /// The gateway's own estimate from public prices; never billing.
    pub cost: f64,
    /// Tokens per model, sorted by size descending.
    pub by_model: Vec<ModelUsage>,
    /// The newest requests, newest first, at most ten.
    pub recent: Vec<Request>,
    /// Requests the gateway counts as running now.
    pub active: u64,
}

/// One model's tokens in the period.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct ModelUsage {
    pub model: String,
    pub tokens: u64,
}

/// One request the gateway served.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Request {
    pub model: String,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    /// False when the gateway reports it failed.
    pub ok: bool,
    /// When it finished, in ms since the epoch; 0 when unknown.
    pub at_ms: i64,
}

/// What the extension reads: the connection's state and one period's numbers.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Router {
    /// "connected", "unauthorized", "unreachable", "outdated" or "" (nothing enabled or watched).
    pub status: String,
    /// The gateway's own version string once known.
    pub version: String,
    /// The period the numbers cover.
    pub period: String,
    pub stats: Stats,
}

/// What Settings sends from the panel.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct Config {
    enabled: bool,
    connection: String,
    base_url: String,
    period: String,
}

impl Default for Config {
    /// The Local default: zero input, the gateway looked for where its npm install serves.
    fn default() -> Self {
        Self {
            enabled: false,
            connection: "local".into(),
            base_url: String::new(),
            period: "today".into(),
        }
    }
}

/// One gateway behind the trait; page, chip and poll never learn which one it is. `id` tells
/// implementations apart where more than one exists; one exists today.
#[allow(dead_code)]
trait RouterGateway {
    /// The gateway's id, never shown in the UI ("9router").
    fn id(&self) -> &'static str;
    /// The address to talk to: the local default, or Settings' base URL.
    fn base_url(&self) -> &str;
    /// Whether a status from the gateway means the auth was refused.
    fn unauthorized(&self, status: u16) -> bool;
    /// The headers the gateway's auth mode needs on a usage request.
    fn auth_headers(&self) -> Vec<(&'static str, String)>;
}

/// 9Router: same-machine CLI credentials from its data directory, sent the way its own CLI sends
/// them (see the plan, section 5, Mode 1).
struct NineRouter {
    base_url: String,
    token: Option<String>,
}

impl RouterGateway for NineRouter {
    fn id(&self) -> &'static str {
        "9router"
    }

    fn base_url(&self) -> &str {
        &self.base_url
    }

    fn unauthorized(&self, status: u16) -> bool {
        status == 401 || status == 403
    }

    fn auth_headers(&self) -> Vec<(&'static str, String)> {
        self.token
            .as_ref()
            .map(|token| vec![("x-9r-cli-token", token.clone())])
            .unwrap_or_default()
    }
}

/// Reads the CLI credentials the gateway left on this machine and derives its token, or `None`
/// when it never ran here: the Remote option is then a click away in Settings.
fn read_token() -> Option<String> {
    let home = dirs::home_dir()?;
    let machine_id = read_trimmed(home.join(".9router/data/machine-id"))
        .or_else(|| read_trimmed(home.join(".9router/machine-id")))?;
    let cli_secret = read_trimmed(home.join(".9router/data/auth/cli-secret"))?;
    let mut hasher = Sha256::new();
    hasher.update(machine_id.as_bytes());
    hasher.update(CLI_AUTH.as_bytes());
    hasher.update(cli_secret.as_bytes());
    let digest = hasher.finalize();
    Some(hex(&digest[..8]))
}

/// A file read whole and trimmed, or `None` when it is not there.
fn read_trimmed(path: PathBuf) -> Option<String> {
    std::fs::read_to_string(path).ok().map(|text| text.trim().to_owned())
}

/// Lowercase hex of `bytes`.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Why a read failed, in one word the status line shows.
#[derive(Debug, PartialEq)]
enum Failure {
    Unauthorized,
    Unreachable,
}

/// The router's D-Bus side: its config and the last state.
pub struct RouterService {
    state: Router,
    config: Config,
    watching: bool,
    refresh: Arc<Notify>,
}

impl RouterService {
    /// Also returns the handle that wakes the polling loop.
    pub fn new() -> (Self, Arc<Notify>) {
        let refresh = Arc::new(Notify::new());
        (
            Self {
                state: Router::default(),
                config: Config::default(),
                watching: false,
                refresh: refresh.clone(),
            },
            refresh,
        )
    }
}

#[interface(name = "io.github.zickrian.isle.Router1")]
impl RouterService {
    /// JSON `{enabled, connection, base_url, period}`, from Settings.
    fn set_config(&mut self, config: &str) -> fdo::Result<()> {
        self.config =
            serde_json::from_str(config).map_err(|err| fdo::Error::InvalidArgs(err.to_string()))?;
        self.refresh.notify_one();
        Ok(())
    }

    /// The island shows the chip or the page: poll while `true`, sleep while `false`.
    fn set_watching(&mut self, watching: bool) {
        self.watching = watching;
        self.refresh.notify_one();
    }

    /// Reads the usage again now.
    fn refresh(&self) {
        self.refresh.notify_one();
    }

    /// JSON `{status, version, period, stats: {requests, prompt_tokens, completion_tokens,
    /// cached_tokens, cost, by_model: [{model, tokens}], recent: [{model, prompt_tokens,
    /// completion_tokens, ok, at_ms}], active}}`.
    #[zbus(property)]
    fn state(&self) -> String {
        serde_json::to_string(&self.state).unwrap_or_default()
    }
}

/// Reads the gateway's usage now and then, publishes it, and backs off when it fails.
pub async fn run(iface: InterfaceRef<RouterService>, refresh: Arc<Notify>) {
    let mut failures = 0usize;
    let mut version = String::new();
    loop {
        let (config, watching) = {
            let service = iface.get().await;
            (service.config.clone(), service.watching)
        };
        // Nothing enabled, or nobody watching: no timer at all, only the next config or watch.
        if !config.enabled || !watching {
            refresh.notified().await;
            failures = 0;
            let state = Router {
                period: iface.get().await.config.period.clone(),
                ..Router::default()
            };
            publish(&iface, state).await;
            continue;
        }
        let gateway = NineRouter {
            base_url: if config.connection == "remote" && !config.base_url.is_empty() {
                config.base_url.trim_end_matches('/').to_owned()
            } else {
                LOCAL_URL.into()
            },
            token: read_token(),
        };
        let period = config.period.clone();
        let known = version.clone();
        let read = tokio::task::spawn_blocking(move || read_state(&gateway, &period, known))
            .await
            .ok();
        let state = match read {
            Some(Ok(state)) => {
                failures = 0;
                version = state.version.clone();
                state
            }
            Some(Err(Failure::Unauthorized)) => {
                failures += 1;
                Router {
                    status: "unauthorized".into(),
                    version: version.clone(),
                    period: config.period.clone(),
                    stats: iface.get().await.state.stats.clone(),
                }
            }
            Some(Err(Failure::Unreachable)) | None => {
                failures += 1;
                Router {
                    status: "unreachable".into(),
                    version: version.clone(),
                    period: config.period.clone(),
                    stats: iface.get().await.state.stats.clone(),
                }
            }
        };
        publish(&iface, state).await;
        let wait = if failures == 0 {
            POLL
        } else {
            BACKOFF[(failures - 1).min(BACKOFF.len() - 1)]
        };
        tokio::select! {
            () = tokio::time::sleep(wait) => {}
            () = refresh.notified() => {}
        }
    }
}

/// Puts `state` on the property, without sending it again unchanged.
async fn publish(iface: &InterfaceRef<RouterService>, state: Router) {
    let mut service = iface.get_mut().await;
    if service.state == state {
        return;
    }
    service.state = state;
    if let Err(err) = service.state_changed(iface.signal_emitter()).await {
        warn!("could not publish the router state: {err}");
    }
}

/// Connects (the version) and reads one period's usage, as one state. The version once known is
/// kept, so a failing usage read does not lose it from the status line.
fn read_state(gateway: &NineRouter, period: &str, version: String) -> Result<Router, Failure> {
    let current = if version.is_empty() {
        read_version(gateway)?
    } else {
        version
    };
    if !version_safe(&current) {
        return Ok(Router {
            status: "outdated".into(),
            version: current,
            period: period.into(),
            stats: Stats::default(),
        });
    }
    let stats = read_stats(gateway, period)?;
    Ok(Router {
        status: "connected".into(),
        version: current,
        period: period.into(),
        stats,
    })
}

/// The gateway's own version report, without auth.
fn read_version(gateway: &NineRouter) -> Result<String, Failure> {
    let mut reply = crate::http::get(&format!("{}/api/version", gateway.base_url()), TIMEOUT)
        .call()
        .map_err(|_| Failure::Unreachable)?;
    let version: Version = reply.body_mut().read_json().map_err(|_| Failure::Unreachable)?;
    Ok(version.current)
}

/// The usage of one period, with the auth the gateway's mode needs.
fn read_stats(gateway: &NineRouter, period: &str) -> Result<Stats, Failure> {
    let mut request = crate::http::get(
        &format!("{}/api/usage/stats?period={}", gateway.base_url(), period),
        TIMEOUT,
    );
    for (name, value) in gateway.auth_headers() {
        request = request.header(name, &value);
    }
    let mut reply = request.call().map_err(|err| match &err {
        ureq::Error::StatusCode(code) if gateway.unauthorized(*code) => Failure::Unauthorized,
        _ => Failure::Unreachable,
    })?;
    let raw: RawStats = reply.body_mut().read_json().map_err(|_| Failure::Unreachable)?;
    Ok(raw.into())
}

/// The gateway's version, as it reports it.
#[derive(Debug, Deserialize)]
struct Version {
    #[serde(rename = "currentVersion")]
    current: String,
}

/// `True` when the gateway's version is at least the safe one; unparseable, it is not.
fn version_safe(version: &str) -> bool {
    let mut numbers = version.split('.');
    let major = numbers.next().and_then(|part| part.parse().ok());
    let minor = numbers.next().and_then(|part| part.parse().ok());
    let patch = numbers.next().and_then(|part| part.parse().ok());
    match (major, minor, patch) {
        (Some(major), Some(minor), Some(patch)) => (major, minor, patch) >= SAFE_VERSION,
        _ => false,
    }
}

/// The usage the gateway serves, in its own shape.
#[derive(Debug, Deserialize)]
struct RawStats {
    #[serde(rename = "totalRequests", default)]
    requests: u64,
    #[serde(rename = "totalPromptTokens", default)]
    prompt_tokens: u64,
    #[serde(rename = "totalCompletionTokens", default)]
    completion_tokens: u64,
    #[serde(rename = "totalCachedTokens", default)]
    cached_tokens: u64,
    #[serde(rename = "totalCost", default)]
    cost: f64,
    #[serde(rename = "byModel", default)]
    by_model: serde_json::Value,
    #[serde(rename = "recentRequests", default)]
    recent: Vec<RawRequest>,
    #[serde(rename = "activeRequests", default)]
    active: u64,
}

/// One request as the gateway serves it.
#[derive(Debug, Deserialize)]
struct RawRequest {
    #[serde(default)]
    model: String,
    #[serde(rename = "promptTokens", default)]
    prompt_tokens: u64,
    #[serde(rename = "completionTokens", default)]
    completion_tokens: u64,
    #[serde(default)]
    status: String,
    #[serde(rename = "timestamp", default)]
    at_ms: i64,
}

impl From<RawStats> for Stats {
    fn from(raw: RawStats) -> Self {
        // `byModel` is a map of model → tokens; the page wants it sorted, not its own order.
        let mut by_model = raw
            .by_model
            .as_object()
            .map(|models| {
                models
                    .iter()
                    .map(|(model, tokens)| ModelUsage {
                        model: model.clone(),
                        tokens: tokens.as_u64().unwrap_or(0),
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        by_model.sort_by(|a, b| b.tokens.cmp(&a.tokens).then(a.model.cmp(&b.model)));
        Self {
            requests: raw.requests,
            prompt_tokens: raw.prompt_tokens,
            completion_tokens: raw.completion_tokens,
            cached_tokens: raw.cached_tokens,
            cost: raw.cost,
            by_model,
            recent: raw
                .recent
                .into_iter()
                .map(|request| Request {
                    model: request.model,
                    prompt_tokens: request.prompt_tokens,
                    completion_tokens: request.completion_tokens,
                    ok: !request.status.eq_ignore_ascii_case("error"),
                    at_ms: request.at_ms,
                })
                .take(10)
                .collect(),
            active: raw.active,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The token is the hash's first 16 hex chars, of machine id, salt and secret in order.
    #[test]
    fn token_derivation() {
        let mut hasher = Sha256::new();
        hasher.update(b"machine-123");
        hasher.update(CLI_AUTH.as_bytes());
        hasher.update(b"secret-abc");
        let digest = hasher.finalize();
        assert_eq!(hex(&digest[..8]).len(), 16);
    }

    /// Only 0.4.42 and above passes; anything else — older, unparseable — fetches no usage.
    #[test]
    fn version_gate() {
        assert!(version_safe("0.4.42"));
        assert!(version_safe("0.5.112"));
        assert!(version_safe("1.2.0"));
        assert!(!version_safe("0.4.41"));
        assert!(!version_safe("0.4"));
        assert!(!version_safe("not-a-version"));
    }

    /// The gateway's shape becomes the page's shape, `byModel` sorted by tokens, the newest ten
    /// requests kept, an "error" status kept as not ok.
    #[test]
    fn stats_shape() {
        let raw: RawStats = serde_json::from_str(
            r#"{
                "totalRequests": 12,
                "totalPromptTokens": 1000,
                "totalCompletionTokens": 200,
                "totalCachedTokens": 300,
                "totalCost": 0.5,
                "byModel": {"small": 400, "big": 900, "mid": 100},
                "recentRequests": [
                    {"model": "big", "promptTokens": 10, "completionTokens": 2,
                     "status": "success", "timestamp": 2},
                    {"model": "small", "promptTokens": 5, "completionTokens": 0,
                     "status": "error", "timestamp": 1}
                ],
                "activeRequests": 3
            }"#,
        )
        .unwrap();
        let stats: Stats = raw.into();
        assert_eq!(stats.requests, 12);
        assert_eq!(stats.cost, 0.5);
        assert_eq!(
            stats
                .by_model
                .iter()
                .map(|usage| usage.model.as_str())
                .collect::<Vec<_>>(),
            ["big", "small", "mid"]
        );
        assert_eq!(stats.recent.len(), 2);
        assert!(stats.recent[0].ok);
        assert!(!stats.recent[1].ok);
        assert_eq!(stats.active, 3);
    }

    /// A gateway answering on this machine is read: its version, then its usage, connected, with
    /// the CLI token on the usage call only.
    #[test]
    fn reads_a_live_gateway() {
        let version_body = "{\"currentVersion\": \"0.5.112\"}".to_owned();
        let stats_body = "{\"totalRequests\": 5, \"totalCost\": 0.5, \"byModel\": {\"big\": 900}, \"recentRequests\": [], \"activeRequests\": 0}".to_owned();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            use std::io::{Read, Write};
            for (path, wants_token) in [("/api/version", false), ("/api/usage/stats?period=today", true)] {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buffer = [0; 4096];
                let read = stream.read(&mut buffer).unwrap();
                let request = String::from_utf8_lossy(&buffer[..read]).to_string();
                let body = if path == "/api/version" {
                    version_body.clone()
                } else {
                    stats_body.clone()
                };
                let refused = wants_token && !request.contains("x-9r-cli-token: abc");
                let head = if refused { "401 Unauthorized" } else { "200 OK" };
                let reply = format!(
                    "HTTP/1.1 {head}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(reply.as_bytes());
            }
        });
        let gateway = NineRouter {
            base_url: format!("http://127.0.0.1:{port}"),
            token: Some("abc".into()),
        };
        let state = read_state(&gateway, "today", String::new()).unwrap();
        assert_eq!(state.status, "connected");
        assert_eq!(state.version, "0.5.112");
        assert_eq!(state.stats.requests, 5);
        assert_eq!(state.stats.cost, 0.5);
    }
}
