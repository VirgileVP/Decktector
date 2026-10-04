//! Where the remote server gets the UI it hands to browsers.
//!
//! A packaged app ships the built renderer as a bundle resource (see
//! [`AssetSource::Directory`]) and serves it directly. Under `tauri dev`
//! there is no bundle yet — Vite owns the assets on its own port — so the
//! server proxies to it (see [`AssetSource::DevServer`]).

use axum::body::Body;
use axum::extract::Request;
use axum::response::{IntoResponse, Response};
use axum::routing::{any, Router};
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use std::path::PathBuf;
use tower_http::services::{ServeDir, ServeFile};

/// Where the remote server gets the UI it hands to browsers.
#[derive(Debug, Clone)]
pub enum AssetSource {
    /// A built bundle on disk — the packaged app's `web` resource.
    Directory(PathBuf),
    /// Proxy to the Vite dev server, which owns the assets during
    /// `tauri dev`.
    ///
    /// Proxied rather than redirected: a redirect moves the browser onto
    /// Vite's origin, and everything the page then does is measured against
    /// *that* origin instead of this server's. The WebSocket URL is built
    /// from `location`, so it would aim at Vite's port and never reach the
    /// engine; the artwork/waveform fetches would become cross-origin; and
    /// the token cookie, set on this server's origin, would stop being
    /// sent. Proxying keeps one origin, so dev behaves like a packaged
    /// build instead of needing its own special cases on the renderer side.
    DevServer(u16),
}

/// Builds the router that answers every path with the chosen asset source.
///
/// Unguarded by itself — the caller gates it behind the token check, the
/// same as every other remote route, so the whole surface sits behind one
/// gate rather than assets being an unauthenticated side door.
pub fn asset_router(source: AssetSource) -> Router {
    match source {
        AssetSource::Directory(directory) => {
            let index = directory.join("index.html");
            // A path with no matching file falls back to `index.html` — the
            // UI is a single page, so this is what makes a hard refresh on
            // e.g. `/?token=...` work instead of 404ing. `ServeDir` itself
            // rejects any path that normalizes outside `directory` — that
            // guard is load-bearing on a LAN-facing server, and tower-http
            // already gets it right, so it is not reimplemented here.
            let serve_dir = ServeDir::new(directory).not_found_service(ServeFile::new(index));
            Router::new().fallback_service(serve_dir)
        }
        AssetSource::DevServer(port) => {
            let client: DevServerClient = Client::builder(TokioExecutor::new()).build_http();
            Router::new().fallback(any(move |request: Request| {
                proxy_to_dev_server(client.clone(), port, request)
            }))
        }
    }
}

/// Plain-HTTP client used only to reach Vite on loopback — no TLS, since
/// the dev server it talks to is on this same machine.
type DevServerClient = Client<HttpConnector, Body>;

/// Forwards a request to the Vite dev server and returns its response
/// verbatim, so the browser never leaves this server's origin.
async fn proxy_to_dev_server(client: DevServerClient, port: u16, mut request: Request) -> Response {
    let path_and_query = request
        .uri()
        .path_and_query()
        .map(|value| value.as_str())
        .unwrap_or("/");

    let upstream = match format!("http://127.0.0.1:{port}{path_and_query}").parse() {
        Ok(uri) => uri,
        Err(error) => {
            tracing::error!(%error, port, "failed to build the Vite dev server URL");
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "bad dev server URL",
            )
                .into_response();
        }
    };
    *request.uri_mut() = upstream;
    // Vite checks `Host` against its allowed-hosts list; the client's own
    // value names this server, which Vite has never heard of.
    request.headers_mut().insert(
        axum::http::header::HOST,
        axum::http::HeaderValue::from_str(&format!("127.0.0.1:{port}"))
            .expect("a host built from a port is always a valid header value"),
    );

    match client.request(request).await {
        Ok(response) => response.map(Body::new).into_response(),
        Err(error) => {
            tracing::warn!(%error, port, "the Vite dev server did not answer; is `npm run dev` running?");
            (
                axum::http::StatusCode::BAD_GATEWAY,
                "the Vite dev server is not reachable",
            )
                .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request as HttpRequest, StatusCode};
    use tower::ServiceExt;

    use super::*;

    #[tokio::test]
    async fn a_path_escaping_the_asset_root_is_rejected() {
        let directory = tempfile::tempdir().expect("temp dir creates");
        std::fs::write(directory.path().join("index.html"), "<html></html>")
            .expect("index.html writes");

        let router = asset_router(AssetSource::Directory(directory.path().to_path_buf()));

        let response = router
            .oneshot(
                HttpRequest::builder()
                    .uri("/../../../../../../etc/passwd")
                    .body(Body::empty())
                    .expect("request builds"),
            )
            .await
            .expect("the router itself does not error");

        assert_ne!(
            response.status(),
            StatusCode::OK,
            "a path traversal attempt must never be served"
        );
    }

    #[tokio::test]
    async fn an_unknown_path_falls_back_to_index_html() {
        let directory = tempfile::tempdir().expect("temp dir creates");
        std::fs::write(
            directory.path().join("index.html"),
            "<html>decktector</html>",
        )
        .expect("index.html writes");

        let router = asset_router(AssetSource::Directory(directory.path().to_path_buf()));

        let response = router
            .oneshot(
                HttpRequest::builder()
                    .uri("/some/client/route")
                    .body(Body::empty())
                    .expect("request builds"),
            )
            .await
            .expect("the router itself does not error");

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body reads");
        assert_eq!(body, "<html>decktector</html>".as_bytes());
    }
}
