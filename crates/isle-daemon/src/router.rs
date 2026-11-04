// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 zickrian

//! LLM router usage for the island: what an AI gateway running here or over the network reports —
//! requests, tokens in and out, and the cost it estimates. Everything behind one trait, so another
//! gateway later is one implementation and a Settings entry. The first gateway is 9Router, read the
//! way its own CLI reads it on this machine.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use keyring::{Entry, Error as KeyringError};
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
const KEYRING_SERVICE: &str = "io.github.zickrian.isle";
const REMOTE_PASSWORD: &str = "router-remote-password";
const REMOTE_SESSION: &str = "router-remote-session";
const PROXY_KEY: &str = "router-proxy-key";

/// One period's numbers, as the chip and the page draw them.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
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
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct ModelUsage {
    pub model: String,
    pub tokens: u64,
}

/// One request the gateway served.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
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
    /// "connected", "unauthorized", "unreachable", "outdated", a setup error, or "" when idle.
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
    #[serde(default = "default_auth")]
    auth: String,
    base_url: String,
    period: String,
}

impl Default for Config {
    /// The Local default: zero input, the gateway looked for where its npm install serves.
    fn default() -> Self {
        Self {
            enabled: false,
            connection: "local".into(),
            auth: "password".into(),
            base_url: String::new(),
            period: "today".into(),
        }
    }
}

fn default_auth() -> String {
    "password".into()
}

/// A keyring entry, named so neither the secret nor its type enters GSettings or a plain file.
fn credential_entry(kind: &str) -> Result<Entry, keyring::Error> {
    let session_account = kind
        .strip_prefix("remote-session:")
        .map(|host| format!("{REMOTE_SESSION}-{host}"));
    let account = match kind {
        "remote-password" => REMOTE_PASSWORD.to_owned(),
        "proxy-key" => PROXY_KEY.to_owned(),
        _ => session_account.ok_or_else(|| {
            KeyringError::Invalid("unknown router credential".into(), kind.into())
        })?,
    };
    Entry::new(KEYRING_SERVICE, &account)
}

fn read_credential(kind: &str) -> Result<Option<String>, keyring::Error> {
    match credential_entry(kind)?.get_password() {
        Ok(secret) => Ok(Some(secret)),
        Err(KeyringError::NoEntry) => Ok(None),
        Err(error) => Err(error),
    }
}

fn write_credential(kind: &str, secret: &str) -> Result<(), keyring::Error> {
    credential_entry(kind)?.set_password(secret)?;
    Ok(())
}

fn clear_one_credential(kind: &str) -> Result<(), keyring::Error> {
    match credential_entry(kind)?.delete_credential() {
        Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
        Err(error) => Err(error),
    }
}

fn clear_credential(kind: &str) -> Result<(), keyring::Error> {
    clear_one_credential(kind)?;
    Ok(())
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
    cookie: Option<String>,
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
        if let Some(token) = &self.token {
            vec![("x-9r-cli-token", token.clone())]
        } else if let Some(cookie) = &self.cookie {
            vec![("Cookie", cookie.clone())]
        } else {
            Vec::new()
        }
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
    std::fs::read_to_string(path)
        .ok()
        .map(|text| text.trim().to_owned())
}

/// Lowercase hex of `bytes`.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// A session is tied to its host, so changing the VPS never sends its cookie to another server.
fn session_key(base_url: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(base_url.trim_end_matches('/').as_bytes());
    let digest = hasher.finalize();
    format!("remote-session:{}", hex(&digest[..8]))
}

/// Why a read failed, in one word the status line shows.
#[derive(Debug, PartialEq)]
enum Failure {
    Unauthorized,
    Unreachable,
    InvalidResponse,
    NeedsLocalSetup,
    NeedsCredentials,
    NeedsConfiguration,
    KeyringUnavailable,
    HttpsRequired,
}

/// The router's D-Bus side: its config and the last state.
pub struct RouterService {
    state: Router,
    config: Config,
    watching: bool,
    refresh: Arc<Notify>,
    credential_revision: AtomicU64,
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
                credential_revision: AtomicU64::new(0),
            },
            refresh,
        )
    }
}

#[interface(name = "io.github.zickrian.isle.Router1")]
impl RouterService {
    /// JSON `{enabled, connection, auth, base_url, period}`, from Settings.
    fn set_config(&mut self, config: &str) -> fdo::Result<()> {
        self.config =
            serde_json::from_str(config).map_err(|err| fdo::Error::InvalidArgs(err.to_string()))?;
        self.refresh.notify_one();
        Ok(())
    }

    /// Saves a Remote password or proxy key in the desktop's secret store.
    async fn set_credential(&self, kind: &str, secret: &str, base_url: &str) -> fdo::Result<()> {
        if secret.is_empty() {
            return Err(fdo::Error::InvalidArgs("credential cannot be empty".into()));
        }
        if !matches!(kind, "remote-password" | "proxy-key") {
            return Err(fdo::Error::InvalidArgs("unknown router credential".into()));
        }
        let kind = kind.to_owned();
        let secret = secret.to_owned();
        let base_url = base_url.to_owned();
        tokio::task::spawn_blocking(move || {
            if kind == "remote-password" {
                clear_one_credential(&session_key(&base_url))?;
            }
            write_credential(&kind, &secret)
        })
        .await
        .map_err(|error| fdo::Error::Failed(error.to_string()))?
        .map_err(|error| fdo::Error::Failed(format!("GNOME Keyring: {error}")))?;
        self.refresh.notify_one();
        self.credential_revision.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// Removes a Remote password or proxy key from the desktop's secret store.
    async fn clear_credential(&self, kind: &str, base_url: &str) -> fdo::Result<()> {
        if !matches!(kind, "remote-password" | "proxy-key") {
            return Err(fdo::Error::InvalidArgs("unknown router credential".into()));
        }
        let kind = kind.to_owned();
        let base_url = base_url.to_owned();
        tokio::task::spawn_blocking(move || -> Result<(), keyring::Error> {
            clear_credential(&kind)?;
            if kind == "remote-password" {
                clear_one_credential(&session_key(&base_url))?;
            }
            Ok(())
        })
        .await
        .map_err(|error| fdo::Error::Failed(error.to_string()))?
        .map_err(|error| fdo::Error::Failed(format!("GNOME Keyring: {error}")))?;
        self.refresh.notify_one();
        self.credential_revision.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// Whether a Remote secret is saved, without returning it to Settings.
    async fn has_credential(&self, kind: &str) -> fdo::Result<bool> {
        if !matches!(kind, "remote-password" | "proxy-key") {
            return Err(fdo::Error::InvalidArgs("unknown router credential".into()));
        }
        let kind = kind.to_owned();
        tokio::task::spawn_blocking(move || read_credential(&kind).map(|secret| secret.is_some()))
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))?
            .map_err(|error| fdo::Error::Failed(format!("GNOME Keyring: {error}")))
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
    let mut endpoint = String::new();
    let mut last_password_login = None;
    let mut credential_revision = 0;
    loop {
        let (config, watching, current_credential_revision) = {
            let service = iface.get().await;
            (
                service.config.clone(),
                service.watching,
                service.credential_revision.load(Ordering::Relaxed),
            )
        };
        if credential_revision != current_credential_revision {
            credential_revision = current_credential_revision;
            last_password_login = None;
        }
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
        let remote = config.connection == "remote";
        let target = if remote {
            config.base_url.trim_end_matches('/')
        } else {
            LOCAL_URL
        };
        let next_endpoint = format!("{}:{}:{}", config.connection, config.auth, target);
        if next_endpoint != endpoint {
            endpoint = next_endpoint;
            version.clear();
            last_password_login = None;
        }
        let allow_login = last_password_login
            .is_none_or(|last: tokio::time::Instant| last.elapsed() >= Duration::from_secs(60));
        let period = config.period.clone();
        let known = version.clone();
        let read_config = config.clone();
        let read = tokio::task::spawn_blocking(move || {
            if !remote {
                let Some(token) = read_token() else {
                    return (Err(Failure::NeedsLocalSetup), None, false, false);
                };
                let gateway = NineRouter {
                    base_url: LOCAL_URL.into(),
                    token: Some(token),
                    cookie: None,
                };
                return (read_state(&gateway, &period, known), None, false, false);
            }
            if read_config.base_url.trim().is_empty() {
                return (Err(Failure::NeedsConfiguration), None, false, false);
            }
            if read_config.auth == "proxy" {
                let result = read_proxy_state(&read_config.base_url, &period);
                return (result, None, false, false);
            }
            let (state, cookie, login_attempted, store_session) =
                read_password_state(&read_config.base_url, &period, known, allow_login);
            (state, cookie, store_session, login_attempted)
        })
        .await
        .ok();
        let (result, session_cookie, store_session, login_attempted) =
            read.unwrap_or((Err(Failure::Unreachable), None, false, false));
        if login_attempted {
            last_password_login = Some(tokio::time::Instant::now());
        }
        if store_session && let Some(cookie) = session_cookie {
            let key = session_key(&config.base_url);
            let _ = tokio::task::spawn_blocking(move || write_credential(&key, &cookie)).await;
        }
        let state = match result {
            Ok(state) => {
                failures = 0;
                version = state.version.clone();
                state
            }
            Err(Failure::Unauthorized) => {
                failures += 1;
                Router {
                    status: "unauthorized".into(),
                    version: version.clone(),
                    period: config.period.clone(),
                    stats: iface.get().await.state.stats.clone(),
                }
            }
            Err(Failure::NeedsCredentials) => Router {
                status: "needs-credentials".into(),
                period: config.period.clone(),
                ..Router::default()
            },
            Err(Failure::NeedsConfiguration) => Router {
                status: "needs-configuration".into(),
                period: config.period.clone(),
                ..Router::default()
            },
            Err(Failure::NeedsLocalSetup) => Router {
                status: "needs-local-setup".into(),
                period: config.period.clone(),
                ..Router::default()
            },
            Err(Failure::KeyringUnavailable) => Router {
                status: "keyring-unavailable".into(),
                period: config.period.clone(),
                ..Router::default()
            },
            Err(Failure::HttpsRequired) => Router {
                status: "https-required".into(),
                period: config.period.clone(),
                ..Router::default()
            },
            Err(Failure::InvalidResponse) => Router {
                status: "invalid-response".into(),
                period: config.period.clone(),
                ..Router::default()
            },
            Err(Failure::Unreachable) => {
                failures += 1;
                Router {
                    status: "unreachable".into(),
                    version: version.clone(),
                    period: config.period.clone(),
                    stats: iface.get().await.state.stats.clone(),
                }
            }
        };
        let authentication_failed = state.status == "unauthorized";
        publish(&iface, state).await;
        let wait = if authentication_failed {
            // A bad password or key should not send repeated auth requests while the user fixes it.
            Duration::from_secs(60)
        } else if failures == 0 {
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

/// Remote 9Router dashboard login, with a keyring-backed session cookie refreshed at most once a minute.
fn read_password_state(
    base_url: &str,
    period: &str,
    version: String,
    allow_login: bool,
) -> (Result<Router, Failure>, Option<String>, bool, bool) {
    let base_url = base_url.trim_end_matches('/');
    if !secure_remote(base_url) {
        return (Err(Failure::HttpsRequired), None, false, false);
    }
    let password = match read_credential("remote-password") {
        Ok(Some(password)) => password,
        Ok(None) => return (Err(Failure::NeedsCredentials), None, false, false),
        Err(_) => return (Err(Failure::KeyringUnavailable), None, false, false),
    };
    let cached = match read_credential(&session_key(base_url)) {
        Ok(cookie) => cookie,
        Err(_) => return (Err(Failure::KeyringUnavailable), None, false, false),
    };
    let version = if version.is_empty() {
        let gateway = NineRouter {
            base_url: base_url.into(),
            token: None,
            cookie: None,
        };
        match read_version(&gateway) {
            Ok(version) => version,
            Err(error) => return (Err(error), cached, false, false),
        }
    } else {
        version
    };
    if !version_safe(&version) {
        return (
            Ok(Router {
                status: "outdated".into(),
                version,
                period: period.into(),
                stats: Stats::default(),
            }),
            cached,
            false,
            false,
        );
    }
    if let Some(cookie) = cached.as_deref() {
        let gateway = NineRouter {
            base_url: base_url.into(),
            token: None,
            cookie: Some(cookie.into()),
        };
        match read_stats(&gateway, period) {
            Ok(stats) => return (Ok(connected(version, period, stats)), cached, false, false),
            Err(Failure::Unreachable) => return (Err(Failure::Unreachable), cached, false, false),
            Err(Failure::Unauthorized) if !allow_login => {
                return (Err(Failure::Unauthorized), cached, false, false);
            }
            Err(Failure::Unauthorized) => {}
            Err(error) => return (Err(error), cached, false, false),
        }
    } else if !allow_login {
        return (Err(Failure::Unauthorized), None, false, false);
    }

    let cookie = match login(base_url, &password) {
        Ok(cookie) => cookie,
        Err(error) => return (Err(error), cached, true, false),
    };
    let gateway = NineRouter {
        base_url: base_url.into(),
        token: None,
        cookie: Some(cookie.clone()),
    };
    match read_stats(&gateway, period) {
        Ok(stats) => (
            Ok(connected(version, period, stats)),
            Some(cookie),
            true,
            true,
        ),
        Err(Failure::Unauthorized) => (Err(Failure::Unauthorized), None, true, false),
        Err(error) => (Err(error), Some(cookie), true, true),
    }
}

fn connected(version: String, period: &str, stats: Stats) -> Router {
    Router {
        status: "connected".into(),
        version,
        period: period.into(),
        stats,
    }
}

/// A remote password or API key must not cross an unencrypted network connection.
fn secure_remote(base_url: &str) -> bool {
    if base_url.starts_with("https://") {
        return true;
    }
    let Some(authority) = base_url
        .strip_prefix("http://")
        .and_then(|url| url.split('/').next())
    else {
        return false;
    };
    let authority = authority.rsplit('@').next().unwrap_or(authority);
    let host = if authority.starts_with('[') {
        authority
            .split(']')
            .next()
            .unwrap_or("")
            .trim_start_matches('[')
    } else {
        authority
            .rsplit_once(':')
            .map_or(authority, |(host, _)| host)
    };
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

/// Dashboard login returns the httpOnly `auth_token` cookie; only its cookie pair is retained.
fn login(base_url: &str, password: &str) -> Result<String, Failure> {
    let mut response = ureq::post(&format!("{base_url}/api/auth/login"))
        .config()
        .timeout_global(Some(TIMEOUT))
        .build()
        .send_json(serde_json::json!({"password": password}))
        .map_err(|error| match &error {
            ureq::Error::StatusCode(401 | 403 | 429) => Failure::Unauthorized,
            _ => Failure::Unreachable,
        })?;
    let cookie = response
        .headers()
        .get("set-cookie")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .filter(|value| value.starts_with("auth_token=") && value.len() > "auth_token=".len())
        .map(str::to_owned)
        .ok_or(Failure::Unauthorized)?;
    // Drain a small JSON body so the connection can be reused; never log it.
    let _ = response.body_mut().read_to_string();
    Ok(cookie)
}

/// Reads the normalized response from the optional remote Isle usage proxy.
fn read_proxy_state(base_url: &str, period: &str) -> Result<Router, Failure> {
    let base_url = base_url.trim_end_matches('/');
    if !secure_remote(base_url) {
        return Err(Failure::HttpsRequired);
    }
    let key = read_credential("proxy-key")
        .map_err(|_| Failure::KeyringUnavailable)?
        .ok_or(Failure::NeedsCredentials)?;
    let mut reply = crate::http::get(&format!("{base_url}/notch?period={period}"), TIMEOUT)
        .header("Authorization", &format!("Bearer {key}"))
        .call()
        .map_err(|error| match &error {
            ureq::Error::StatusCode(401 | 403) => Failure::Unauthorized,
            _ => Failure::Unreachable,
        })?;
    let stats: ProxyStats = reply
        .body_mut()
        .read_json()
        .map_err(|_| Failure::InvalidResponse)?;
    Ok(connected("Compatible proxy".into(), period, stats.into()))
}

/// The gateway's own version report, without auth.
fn read_version(gateway: &NineRouter) -> Result<String, Failure> {
    let mut reply = crate::http::get(&format!("{}/api/version", gateway.base_url()), TIMEOUT)
        .call()
        .map_err(|_| Failure::Unreachable)?;
    let version: Version = reply
        .body_mut()
        .read_json()
        .map_err(|_| Failure::Unreachable)?;
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
    let raw: RawStats = reply
        .body_mut()
        .read_json()
        .map_err(|_| Failure::InvalidResponse)?;
    Ok(raw.into())
}

/// The compatible proxy's public `/notch` response shape, mapped to the Router page's names.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProxyStats {
    requests: u64,
    tokens_in: u64,
    tokens_out: u64,
    tokens_cached: u64,
    estimated_cost: f64,
    models: Vec<ProxyModel>,
    recent: Vec<ProxyRequest>,
    active_requests: u64,
}

#[derive(Debug, Deserialize)]
struct ProxyModel {
    #[serde(rename = "name")]
    model: String,
    tokens: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProxyRequest {
    model: String,
    status: String,
    timestamp: serde_json::Value,
    tokens_in: u64,
    tokens_out: u64,
}

impl From<ProxyStats> for Stats {
    fn from(proxy: ProxyStats) -> Self {
        Self {
            requests: proxy.requests,
            prompt_tokens: proxy.tokens_in,
            completion_tokens: proxy.tokens_out,
            cached_tokens: proxy.tokens_cached,
            cost: proxy.estimated_cost,
            by_model: proxy
                .models
                .into_iter()
                .map(|model| ModelUsage {
                    model: model.model,
                    tokens: model.tokens,
                })
                .collect(),
            recent: proxy
                .recent
                .into_iter()
                .map(|request| Request {
                    model: request.model,
                    prompt_tokens: request.tokens_in,
                    completion_tokens: request.tokens_out,
                    ok: matches!(request.status.as_str(), "ok" | "success"),
                    at_ms: proxy_timestamp_ms(&request.timestamp),
                })
                .collect(),
            active: proxy.active_requests,
        }
    }
}

fn proxy_timestamp_ms(timestamp: &serde_json::Value) -> i64 {
    match timestamp {
        serde_json::Value::Number(number) => number.as_i64().map_or(0, |value| {
            if value.unsigned_abs() < 100_000_000_000 {
                value.saturating_mul(1000)
            } else {
                value
            }
        }),
        serde_json::Value::String(value) => chrono::DateTime::parse_from_rfc3339(value)
            .map(|timestamp| timestamp.timestamp_millis())
            .or_else(|_| {
                value.parse::<i64>().map(|timestamp| {
                    if timestamp.unsigned_abs() < 100_000_000_000 {
                        timestamp.saturating_mul(1000)
                    } else {
                        timestamp
                    }
                })
            })
            .unwrap_or(0),
        _ => 0,
    }
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
            for (path, wants_token) in [
                ("/api/version", false),
                ("/api/usage/stats?period=today", true),
            ] {
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
                let head = if refused {
                    "401 Unauthorized"
                } else {
                    "200 OK"
                };
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
            cookie: None,
        };
        let state = read_state(&gateway, "today", String::new()).unwrap();
        assert_eq!(state.status, "connected");
        assert_eq!(state.version, "0.5.112");
        assert_eq!(state.stats.requests, 5);
        assert_eq!(state.stats.cost, 0.5);
    }
}
