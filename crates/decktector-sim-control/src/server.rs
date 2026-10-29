//! The loopback listener behind the control page.
//!
//! There is no token or other auth: the server binds `127.0.0.1` only, so
//! nothing off this machine can reach it, and it exists in dev builds alone.

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{State, WebSocketUpgrade};
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use decktector_core::sim::SimControl;
use decktector_remote::assets::{asset_router, AssetSource};
use tokio::net::TcpListener;
use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::audio::{audio_router, AudioCatalog};
use crate::session;

/// Port of the control server. Clear of the remote view's 7420, Vite's dev
/// port and the Pro DJ Link range.
pub const SIM_CONTROL_PORT: u16 = 1430;

/// Loopback only — see the module docs.
const BIND_ADDRESS: Ipv4Addr = Ipv4Addr::LOCALHOST;

/// Path of the control page among the served assets.
const PAGE_PATH: &str = "/sim-cdj.html";

/// Serves the control page and its WebSocket for as long as it is alive.
pub struct SimControlServer {
    address: SocketAddr,
    connected_clients: watch::Receiver<usize>,
    serve_task: JoinHandle<()>,
}

impl SimControlServer {
    /// Binds [`SIM_CONTROL_PORT`] on loopback and starts serving. Fails when
    /// the port cannot be bound, e.g. another instance already holds it.
    /// `audio` lists the files `GET /audio/{track_id}` may serve.
    pub async fn start(
        control: SimControl,
        assets: AssetSource,
        audio: AudioCatalog,
    ) -> anyhow::Result<Arc<Self>> {
        let address = SocketAddr::from((BIND_ADDRESS, SIM_CONTROL_PORT));
        Self::start_on(address, control, assets, audio).await
    }

    /// [`Self::start`] on a chosen address; tests bind port 0.
    pub async fn start_on(
        address: SocketAddr,
        control: SimControl,
        assets: AssetSource,
        audio: AudioCatalog,
    ) -> anyhow::Result<Arc<Self>> {
        let listener = TcpListener::bind(address).await.map_err(|error| {
            anyhow::anyhow!("failed to bind the sim control server to {address}: {error}")
        })?;
        let address = listener.local_addr()?;

        let (client_count, connected_clients) = watch::channel(0);
        let router = Router::new()
            .route("/ws", get(ws_handler))
            .with_state(ConnectionContext {
                control,
                audio: audio.clone(),
                client_count,
            })
            // Before the assets, whose fallback would answer any other path.
            .merge(audio_router(audio))
            .merge(asset_router(assets));

        let serve_task = tokio::spawn(async move {
            if let Err(error) = axum::serve(listener, router).await {
                tracing::error!(%error, "the sim control server stopped unexpectedly");
            }
        });

        Ok(Arc::new(Self {
            address,
            connected_clients,
            serve_task,
        }))
    }

    /// Where to open the control page.
    pub fn page_url(&self) -> String {
        format!("http://{}{PAGE_PATH}", self.address)
    }

    /// Whether a page is connected, or connects within `grace`. Lets the app
    /// open the browser only when no already-open page reconnects by itself.
    pub async fn wait_for_client(&self, grace: Duration) -> bool {
        let mut connected_clients = self.connected_clients.clone();
        tokio::time::timeout(grace, connected_clients.wait_for(|count| *count > 0))
            .await
            .is_ok_and(|result| result.is_ok())
    }
}

impl Drop for SimControlServer {
    fn drop(&mut self) {
        self.serve_task.abort();
    }
}

/// What a WebSocket upgrade needs to serve its connection.
#[derive(Clone)]
struct ConnectionContext {
    control: SimControl,
    audio: AudioCatalog,
    client_count: watch::Sender<usize>,
}

async fn ws_handler(
    State(context): State<ConnectionContext>,
    upgrade: WebSocketUpgrade,
) -> Response {
    upgrade.on_upgrade(move |socket| async move {
        let _connection = ClientGuard::register(context.client_count);
        session::run(socket, context.control, context.audio).await;
    })
}

/// Counts a connection in the client total for as long as it lives.
struct ClientGuard(watch::Sender<usize>);

impl ClientGuard {
    fn register(client_count: watch::Sender<usize>) -> Self {
        client_count.send_modify(|count| *count += 1);
        Self(client_count)
    }
}

impl Drop for ClientGuard {
    fn drop(&mut self) {
        self.0.send_modify(|count| *count -= 1);
    }
}
