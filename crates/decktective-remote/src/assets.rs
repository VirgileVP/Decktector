//! Where the remote server gets the UI it hands to browsers.
//!
//! A packaged app ships the built renderer as a bundle resource (see
//! [`AssetSource::Directory`]) and serves it directly. Under `tauri dev`
//! there is no bundle yet — Vite owns the assets on its own port — so the
//! server instead redirects there (see [`AssetSource::DevServer`]).

use axum::extract::Request;
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{any, Router};
use std::path::PathBuf;
use tower_http::services::{ServeDir, ServeFile};

/// Where the remote server gets the UI it hands to browsers.
#[derive(Debug, Clone)]
pub enum AssetSource {
    /// A built bundle on disk — the packaged app's `web` resource.
    Directory(PathBuf),
    /// Redirect to the Vite dev server, which owns the assets during
    /// `tauri dev`. Only the port is fixed at startup — the host is taken
    /// from each request's own `Host` header, not hardcoded to `localhost`,
    /// so a phone that reached the remote server over the LAN is sent back
    /// to that same LAN address for Vite's assets rather than to itself.
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
            Router::new().fallback(any(move |request: Request| redirect_to_dev_server(port, request)))
        }
    }
}

/// Redirects to the same host (from the request's own `Host` header), the
/// Vite dev port, and the same path and query — so the token survives the
/// hop and a LAN client is sent back to the machine it already reached.
async fn redirect_to_dev_server(port: u16, request: Request) -> Response {
    let host = request
        .headers()
        .get(axum::http::header::HOST)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(':').next())
        .unwrap_or("localhost");
    let path_and_query = request
        .uri()
        .path_and_query()
        .map(|value| value.as_str())
        .unwrap_or("/");
    Redirect::temporary(&format!("http://{host}:{port}{path_and_query}")).into_response()
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
        std::fs::write(directory.path().join("index.html"), "<html>decktective</html>")
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
        assert_eq!(body, "<html>decktective</html>".as_bytes());
    }
}
