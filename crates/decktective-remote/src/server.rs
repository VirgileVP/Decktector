//! HTTP/WebSocket server that hands the engine's state to remote browsers.
//!
//! [`RemoteService`] owns the listener's lifecycle — starting, stopping and
//! restarting it to match whatever [`RemoteConfig`] the settings panel last
//! applied — the same shape `TimecodeService` uses for its own generator.
//! The token gate is a guard against casual access on a shared venue
//! network, not hardened auth: anyone who can sniff the LAN or read the QR
//! code gets in, same as anyone standing at the booth.

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex, MutexGuard};

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Path, Query, Request, State, WebSocketUpgrade};
use axum::http::header;
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use decktective_core::Engine;
use rand::Rng;
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::RecvError;
use tokio::task::JoinHandle;
use ts_rs::TS;

use crate::assets::{asset_router, AssetSource};
use crate::hub::RemoteHub;

/// Default port for the remote view. Picked well clear of the Pro DJ Link
/// range and of Vite's dev port.
pub const DEFAULT_REMOTE_PORT: u16 = 7420;

/// How many status updates a lagging subscriber may miss before it has to
/// re-hydrate from [`RemoteService::status`].
const STATUS_BUFFER: usize = 16;

/// Length, in bytes, of the access token before hex-encoding — 8 bytes
/// becomes the 16 hex characters handed out in the URL and QR code.
const TOKEN_BYTES: usize = 8;

/// Settings-panel-owned configuration for the remote server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "RemoteConfig.ts")]
pub struct RemoteConfig {
    /// Off by default: the server never opens a port the user did not ask for.
    pub enabled: bool,
    pub port: u16,
    /// Name of the network adapter to bind to, e.g. `"eth0"` — picked from
    /// `list_network_interfaces`, not typed in. Empty binds every interface
    /// this machine has, which is also the fallback when the named adapter
    /// cannot be resolved (unplugged, renamed) — the server stays reachable
    /// rather than failing over an adapter picked in a previous session.
    pub interface: String,
}

/// What the settings panel renders, and what remote browsers are told.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "RemoteStatus.ts")]
pub struct RemoteStatus {
    pub listening: bool,
    pub port: u16,
    /// Access token the client must present, regenerated every time the
    /// server starts. `None` while the server is off.
    pub token: Option<String>,
    /// Number of browsers currently connected, for the settings panel.
    pub client_count: u32,
    /// Set when the last start attempt failed, e.g. the port is taken.
    pub error: Option<String>,
}

/// State a request needs to validate its token and reach the running hub.
struct Inner {
    config: RemoteConfig,
    token: Option<String>,
    client_count: u32,
    listener: Option<JoinHandle<()>>,
    last_status: RemoteStatus,
}

impl Inner {
    fn initial() -> Self {
        let status = RemoteStatus {
            listening: false,
            port: DEFAULT_REMOTE_PORT,
            token: None,
            client_count: 0,
            error: None,
        };
        Self {
            config: RemoteConfig {
                enabled: false,
                port: DEFAULT_REMOTE_PORT,
                interface: String::new(),
            },
            token: None,
            client_count: 0,
            listener: None,
            last_status: status,
        }
    }
}

/// Starts, stops and restarts the remote listener to match the applied
/// [`RemoteConfig`], and answers `/ws`, `/health` and the asset routes for
/// whoever connects.
pub struct RemoteService {
    hub: Arc<RemoteHub>,
    assets: AssetSource,
    inner: Mutex<Inner>,
    status_tx: broadcast::Sender<RemoteStatus>,
}

impl RemoteService {
    pub fn new(engine: Arc<dyn Engine>, assets: AssetSource) -> Arc<Self> {
        Arc::new(Self {
            hub: RemoteHub::new(engine),
            assets,
            inner: Mutex::new(Inner::initial()),
            status_tx: broadcast::channel(STATUS_BUFFER).0,
        })
    }

    /// The hub feeding this service — the Tauri shell spawns [`RemoteHub::run`]
    /// on it once, at startup, alongside the other engine-driven services.
    pub fn hub(&self) -> Arc<RemoteHub> {
        self.hub.clone()
    }

    /// Applies a new config: stops whatever listener is running, then starts
    /// a fresh one if `config.enabled`. A bind failure (e.g. the port is
    /// already in use) is reported through [`RemoteStatus::error`] rather
    /// than returned — the app must keep running either way.
    pub async fn set_config(self: Arc<Self>, config: RemoteConfig) -> anyhow::Result<()> {
        self.stop_listener();

        if !config.enabled {
            self.publish_status(
                RemoteStatus {
                    listening: false,
                    port: config.port,
                    token: None,
                    client_count: 0,
                    error: None,
                },
                config,
            );
            return Ok(());
        }

        let token = generate_token();
        {
            let mut inner = self.lock();
            inner.token = Some(token.clone());
            inner.client_count = 0;
        }

        let router = build_router(self.clone());
        let address = SocketAddr::from((bind_ipv4(&config.interface), config.port));
        match TcpListener::bind(address).await.and_then(|listener| {
            let bound_port = listener.local_addr()?.port();
            Ok((listener, bound_port))
        }) {
            Ok((listener, bound_port)) => {
                let handle = tokio::spawn(async move {
                    if let Err(error) = axum::serve(listener, router).await {
                        tracing::error!(%error, "remote server stopped unexpectedly");
                    }
                });
                self.lock().listener = Some(handle);
                self.publish_status(
                    RemoteStatus {
                        listening: true,
                        // The port actually bound, not the requested one —
                        // they differ when the caller asked for port 0 (an
                        // OS-assigned port, as tests do).
                        port: bound_port,
                        token: Some(token),
                        client_count: 0,
                        error: None,
                    },
                    config,
                );
            }
            Err(error) => {
                tracing::warn!(%error, port = config.port, "failed to bind the remote server's port");
                self.lock().token = None;
                self.publish_status(
                    RemoteStatus {
                        listening: false,
                        port: config.port,
                        token: None,
                        client_count: 0,
                        error: Some(error.to_string()),
                    },
                    config,
                );
            }
        }
        Ok(())
    }

    pub fn status(&self) -> RemoteStatus {
        self.lock().last_status.clone()
    }

    /// Subscribes to status changes. Pair with [`Self::status`] to hydrate a
    /// fresh listener — changes published before this call are not replayed.
    pub fn subscribe(&self) -> broadcast::Receiver<RemoteStatus> {
        self.status_tx.subscribe()
    }

    fn stop_listener(&self) {
        let mut inner = self.lock();
        if let Some(handle) = inner.listener.take() {
            handle.abort();
        }
        inner.token = None;
        inner.client_count = 0;
    }

    fn token_matches(&self, provided: Option<&str>) -> bool {
        let inner = self.lock();
        match (&inner.token, provided) {
            (Some(expected), Some(given)) => expected == given,
            _ => false,
        }
    }

    fn client_connected(&self) {
        self.adjust_client_count(|count| count.saturating_add(1));
    }

    fn client_disconnected(&self) {
        self.adjust_client_count(|count| count.saturating_sub(1));
    }

    fn adjust_client_count(&self, adjust: impl FnOnce(u32) -> u32) {
        let mut inner = self.lock();
        inner.client_count = adjust(inner.client_count);
        inner.last_status.client_count = inner.client_count;
        let status = inner.last_status.clone();
        drop(inner);
        let _ = self.status_tx.send(status);
    }

    fn publish_status(&self, status: RemoteStatus, config: RemoteConfig) {
        let mut inner = self.lock();
        inner.config = config;
        inner.last_status = status.clone();
        drop(inner);
        let _ = self.status_tx.send(status);
    }

    /// The bookkeeping mutex is only ever held for a field update, so a
    /// panic while holding it is not recoverable state we want to preserve.
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// The address to bind the listener to: the named adapter's IPv4 address, or
/// every interface (`0.0.0.0`) when none is picked or the named one cannot
/// be resolved (unplugged, renamed) — a stale interface choice must not take
/// the whole remote view down.
fn bind_ipv4(interface: &str) -> Ipv4Addr {
    if interface.is_empty() {
        return Ipv4Addr::UNSPECIFIED;
    }
    decktective_net::resolve_ipv4(interface).unwrap_or_else(|error| {
        tracing::warn!(
            %error,
            interface,
            "failed to resolve the chosen network interface; binding every interface instead"
        );
        Ipv4Addr::UNSPECIFIED
    })
}

fn generate_token() -> String {
    let mut rng = rand::rng();
    (0..TOKEN_BYTES)
        .map(|_| format!("{:02x}", rng.random::<u8>()))
        .collect()
}

/// `/ws`, `/health` and the track routes need `State<Arc<RemoteService>>`,
/// so each group is closed out with `.with_state` first; the asset routes
/// need no state of their own, only the token-gate layer. Once every group
/// is a plain `Router<()>` they merge into one, ready to hand to
/// `axum::serve`. `/health` sits outside the gate (the settings panel
/// probes it to prove the server is up, with no token yet to offer) and
/// `/ws` gates itself inline; the track and asset routes share the one
/// middleware layer below.
fn build_router(service: Arc<RemoteService>) -> Router {
    let engine_routes = Router::new()
        .route("/ws", get(ws_handler))
        .route("/health", get(health_handler))
        .with_state(service.clone());

    let track_routes = Router::new()
        .route("/track/{track_id}/artwork", get(track_artwork_handler))
        .route("/track/{track_id}/waveform", get(track_waveform_handler))
        .with_state(service.clone());

    let gated = track_routes
        .merge(asset_router(service.assets.clone()))
        .layer(middleware::from_fn_with_state(service, require_token));

    engine_routes.merge(gated)
}

async fn health_handler() -> &'static str {
    "ok"
}

/// How long a browser may cache a track's artwork or waveform: both are
/// immutable for a given `track_id`, so there is no reason to ever ask
/// again once fetched.
const IMMUTABLE_CACHE_CONTROL: &str = "public, max-age=31536000, immutable";

async fn track_artwork_handler(
    State(service): State<Arc<RemoteService>>,
    Path(track_id): Path<u32>,
) -> Response {
    let Some(artwork) = service.hub.track(track_id).and_then(|track| track.artwork) else {
        return (axum::http::StatusCode::NOT_FOUND, "no artwork for this track").into_response();
    };
    match decode_data_url(&artwork) {
        Some((content_type, bytes)) => immutable_response(content_type, bytes),
        None => {
            tracing::error!(track_id, "a track's artwork was not a well-formed data: URL");
            (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "malformed artwork").into_response()
        }
    }
}

async fn track_waveform_handler(
    State(service): State<Arc<RemoteService>>,
    Path(track_id): Path<u32>,
) -> Response {
    let Some(waveform) = service.hub.track(track_id).and_then(|track| track.waveform) else {
        return (axum::http::StatusCode::NOT_FOUND, "no waveform for this track").into_response();
    };
    match serde_json::to_vec(&waveform) {
        Ok(body) => immutable_response("application/json".to_string(), body),
        Err(error) => {
            tracing::error!(%error, track_id, "failed to serialize a track's waveform");
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "failed to serialize waveform",
            )
                .into_response()
        }
    }
}

fn immutable_response(content_type: String, body: Vec<u8>) -> Response {
    (
        [
            (header::CONTENT_TYPE, content_type),
            (header::CACHE_CONTROL, IMMUTABLE_CACHE_CONTROL.to_string()),
        ],
        body,
    )
        .into_response()
}

/// Splits a `data:<mime>;base64,<payload>` URL into its MIME type and
/// decoded bytes — the shape [`TrackInfo::artwork`] is documented to carry
/// (see `model.rs`), turned back into a real HTTP response instead of a
/// JSON string riding along on the WebSocket.
///
/// [`TrackInfo::artwork`]: decktective_core::model::TrackInfo::artwork
fn decode_data_url(data_url: &str) -> Option<(String, Vec<u8>)> {
    let rest = data_url.strip_prefix("data:")?;
    let (meta, payload) = rest.split_once(',')?;
    let mime = meta.strip_suffix(";base64")?;
    let bytes = BASE64.decode(payload).ok()?;
    Some((mime.to_string(), bytes))
}

#[derive(Debug, Deserialize)]
struct TokenQuery {
    token: Option<String>,
}

/// Cookie the token is mirrored into after the first request that carries
/// it in the query string — the query string is how the shared link and
/// the QR code carry it, but a browser has no way to repeat a page's own
/// query string onto the `<script src>`/`<link href>` requests that page
/// goes on to make. Without this, the very first asset request for the
/// bundle's JS would 401, and the page would stay blank.
const TOKEN_COOKIE: &str = "decktective_remote_token";

/// The token from the query string if present, otherwise from
/// [`TOKEN_COOKIE`] — the fallback that lets a browser's own follow-up
/// requests (assets, the WebSocket handshake) authenticate without the
/// query string.
fn resolve_token(query: &TokenQuery, headers: &axum::http::HeaderMap) -> Option<String> {
    query.token.clone().or_else(|| cookie_token(headers))
}

fn cookie_token(headers: &axum::http::HeaderMap) -> Option<String> {
    let cookie_header = headers.get(header::COOKIE)?.to_str().ok()?;
    cookie_header.split(';').find_map(|pair| {
        let (name, value) = pair.trim().split_once('=')?;
        (name == TOKEN_COOKIE).then(|| value.to_string())
    })
}

/// `Set-Cookie` header mirroring a query-string token, so the browser's own
/// follow-up requests carry it automatically. `SameSite=Lax` rather than
/// `Strict`: the QR code and shared link both land here as a top-level
/// navigation from outside the site, which `Strict` would refuse to attach
/// the cookie to.
fn token_cookie_header(token: &str) -> Option<axum::http::HeaderValue> {
    axum::http::HeaderValue::from_str(&format!("{TOKEN_COOKIE}={token}; Path=/; SameSite=Lax")).ok()
}

/// Gates the asset routes behind the same token every other remote route
/// requires, so there is one gate for the whole surface rather than assets
/// being an unauthenticated side door into the app.
async fn require_token(
    State(service): State<Arc<RemoteService>>,
    Query(query): Query<TokenQuery>,
    request: Request,
    next: Next,
) -> Response {
    let token = resolve_token(&query, request.headers());
    if !service.token_matches(token.as_deref()) {
        return (axum::http::StatusCode::UNAUTHORIZED, "invalid or missing token").into_response();
    }
    let mut response = next.run(request).await;
    if let Some(token) = query.token.as_deref() {
        if let Some(value) = token_cookie_header(token) {
            response.headers_mut().insert(header::SET_COOKIE, value);
        }
    }
    response
}

async fn ws_handler(
    State(service): State<Arc<RemoteService>>,
    Query(query): Query<TokenQuery>,
    headers: axum::http::HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    let token = resolve_token(&query, &headers);
    if !service.token_matches(token.as_deref()) {
        return (axum::http::StatusCode::UNAUTHORIZED, "invalid or missing token").into_response();
    }
    upgrade.on_upgrade(move |socket| handle_socket(socket, service))
}

/// Sends the snapshot first, then forwards broadcast messages until the
/// browser disconnects. A lag re-hydrates from a fresh snapshot instead of
/// trying to replay what was missed.
async fn handle_socket(mut socket: WebSocket, service: Arc<RemoteService>) {
    service.client_connected();

    let (snapshot, mut receiver) = match service.hub.subscribe() {
        Ok(pair) => pair,
        Err(error) => {
            tracing::error!(%error, "failed to build the initial remote snapshot");
            service.client_disconnected();
            return;
        }
    };

    if socket.send(Message::Text(snapshot.to_string().into())).await.is_err() {
        service.client_disconnected();
        return;
    }

    loop {
        tokio::select! {
            incoming = socket.recv() => {
                match incoming {
                    None | Some(Err(_)) | Some(Ok(Message::Close(_))) => break,
                    // Nothing else the browser sends changes anything — this
                    // view is read-only.
                    Some(Ok(_)) => {}
                }
            }
            outgoing = receiver.recv() => {
                match outgoing {
                    Ok(text) => {
                        if socket.send(Message::Text(text.to_string().into())).await.is_err() {
                            break;
                        }
                    }
                    Err(RecvError::Lagged(missed)) => {
                        tracing::warn!(missed, "a remote client lagged, re-hydrating it from a fresh snapshot");
                        match service.hub.snapshot_text() {
                            Ok(snapshot) => {
                                if socket.send(Message::Text(snapshot.to_string().into())).await.is_err() {
                                    break;
                                }
                            }
                            Err(error) => {
                                tracing::error!(%error, "failed to rebuild the snapshot after a lag");
                                break;
                            }
                        }
                    }
                    Err(RecvError::Closed) => break,
                }
            }
        }
    }

    service.client_disconnected();
}

#[cfg(test)]
mod tests {
    use decktective_core::sim::SimEngine;

    use super::*;

    #[test]
    fn a_freshly_constructed_service_is_off_until_the_settings_turn_it_on() {
        let engine: Arc<dyn Engine> = Arc::new(SimEngine::new(2));
        let service = RemoteService::new(engine, AssetSource::DevServer(1420));

        let status = service.status();

        assert!(!status.listening);
        assert_eq!(status.port, DEFAULT_REMOTE_PORT);
        assert!(status.token.is_none());
    }

    /// A browser has no way to repeat a page's own query string onto the
    /// `<script src>`/`<link href>` requests that page goes on to make, so
    /// a token carried only in the query string leaves every asset request
    /// unauthenticated — the bundle 401s and the page stays blank. The
    /// cookie `require_token` sets on a successful query-string request is
    /// what makes those follow-up requests authenticate at all.
    #[tokio::test]
    async fn an_asset_request_authenticates_via_the_token_cookie_set_by_the_page_load() {
        use axum::body::Body;
        use axum::http::{header, Request as HttpRequest, StatusCode};
        use tower::ServiceExt;

        let directory = tempfile::tempdir().expect("temp dir creates");
        std::fs::write(directory.path().join("index.html"), "<html>hi</html>")
            .expect("index.html writes");

        let engine: Arc<dyn Engine> = Arc::new(SimEngine::new(2));
        let service = RemoteService::new(engine, AssetSource::Directory(directory.path().to_path_buf()));
        service
            .clone()
            .set_config(RemoteConfig {
                enabled: true,
                port: 0,
                interface: String::new(),
            })
            .await
            .expect("bind succeeds");
        let token = service.status().token.expect("a listening service has a token");

        let page_load = build_router(service.clone())
            .oneshot(
                HttpRequest::builder()
                    .uri(format!("/?token={token}"))
                    .body(Body::empty())
                    .expect("request builds"),
            )
            .await
            .expect("router does not error");
        assert_eq!(page_load.status(), StatusCode::OK);
        let set_cookie = page_load
            .headers()
            .get(header::SET_COOKIE)
            .expect("the page load sets the token cookie")
            .to_str()
            .expect("header is valid utf-8")
            .to_string();
        let cookie_value = set_cookie.split(';').next().expect("cookie has a value");

        let asset_request = build_router(service)
            .oneshot(
                HttpRequest::builder()
                    .uri("/")
                    .header(header::COOKIE, cookie_value)
                    .body(Body::empty())
                    .expect("request builds"),
            )
            .await
            .expect("router does not error");
        assert_eq!(asset_request.status(), StatusCode::OK);
    }
}
