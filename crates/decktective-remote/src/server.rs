//! HTTP/WebSocket server that hands the engine's state to remote browsers.
//!
//! [`RemoteService`] owns the listener's lifecycle — starting, stopping and
//! restarting it to match whatever [`RemoteConfig`] the settings panel last
//! applied — the same shape `TimecodeService` uses for its own generator.
//! The token gate is a guard against casual access on a shared venue
//! network, not hardened auth: anyone who can sniff the LAN or read the QR
//! code gets in, same as anyone standing at the booth.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex, MutexGuard};

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Query, State, WebSocketUpgrade};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use decktective_core::Engine;
use rand::Rng;
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::RecvError;
use tokio::task::JoinHandle;
use ts_rs::TS;

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
            },
            token: None,
            client_count: 0,
            listener: None,
            last_status: status,
        }
    }
}

/// Starts, stops and restarts the remote listener to match the applied
/// [`RemoteConfig`], and answers `/ws` and `/health` for whoever connects.
pub struct RemoteService {
    hub: Arc<RemoteHub>,
    inner: Mutex<Inner>,
    status_tx: broadcast::Sender<RemoteStatus>,
}

impl RemoteService {
    pub fn new(engine: Arc<dyn Engine>) -> Arc<Self> {
        Arc::new(Self {
            hub: RemoteHub::new(engine),
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
        let address = SocketAddr::from(([0, 0, 0, 0], config.port));
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

fn generate_token() -> String {
    let mut rng = rand::rng();
    (0..TOKEN_BYTES)
        .map(|_| format!("{:02x}", rng.random::<u8>()))
        .collect()
}

fn build_router(service: Arc<RemoteService>) -> Router {
    Router::new()
        .route("/ws", get(ws_handler))
        .route("/health", get(health_handler))
        .with_state(service)
}

async fn health_handler() -> &'static str {
    "ok"
}

#[derive(Debug, Deserialize)]
struct WsQuery {
    token: Option<String>,
}

async fn ws_handler(
    State(service): State<Arc<RemoteService>>,
    Query(query): Query<WsQuery>,
    upgrade: WebSocketUpgrade,
) -> Response {
    if !service.token_matches(query.token.as_deref()) {
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
