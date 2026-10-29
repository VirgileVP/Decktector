//! The audio files the control page may play.
//!
//! The page decodes each deck's track in the browser, so the server hands the
//! files out over HTTP. It serves only what an [`AudioCatalog`] lists, by
//! track id: nothing from a request is ever turned into a filesystem path, so
//! the route cannot be walked out of the export.

use std::collections::HashMap;
use std::path::PathBuf;

use axum::extract::{Path, Request, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use tower::ServiceExt;
use tower_http::services::ServeFile;

/// URL prefix of the audio route; the track id follows.
const AUDIO_ROUTE_PREFIX: &str = "/audio";

/// Which audio file belongs to which track id.
#[derive(Debug, Clone, Default)]
pub struct AudioCatalog {
    files: HashMap<u32, PathBuf>,
}

impl AudioCatalog {
    /// A catalog of the given `(track id, file)` pairs. When an id repeats,
    /// the last file wins.
    pub fn new(files: impl IntoIterator<Item = (u32, PathBuf)>) -> Self {
        Self {
            files: files.into_iter().collect(),
        }
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    pub fn contains(&self, track_id: u32) -> bool {
        self.files.contains_key(&track_id)
    }

    /// The URL the page fetches the track's audio from, `None` when the
    /// catalog has no file for it.
    pub fn url_for(&self, track_id: u32) -> Option<String> {
        self.contains(track_id)
            .then(|| format!("{AUDIO_ROUTE_PREFIX}/{track_id}"))
    }
}

/// `GET /audio/{track_id}`: the cataloged file, with `Range` support (the
/// browser seeks in it) and a `Content-Type` taken from its extension; 404
/// for an id the catalog does not list.
pub(crate) fn audio_router(catalog: AudioCatalog) -> Router {
    Router::new()
        .route(
            &format!("{AUDIO_ROUTE_PREFIX}/{{track_id}}"),
            get(serve_audio),
        )
        .with_state(catalog)
}

async fn serve_audio(
    State(catalog): State<AudioCatalog>,
    Path(track_id): Path<u32>,
    request: Request,
) -> Response {
    let Some(file) = catalog.files.get(&track_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match ServeFile::new(file).oneshot(request).await {
        Ok(response) => response.into_response(),
        // `ServeFile`'s error type is `Infallible`.
        Err(never) => match never {},
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use axum::body::Body;
    use axum::http::header::{CONTENT_RANGE, CONTENT_TYPE, RANGE};
    use http_body_util::BodyExt;

    const FILE_BYTES: &[u8] = b"0123456789";
    const KNOWN_TRACK_ID: u32 = 42;

    /// Audio extensions a rekordbox library holds, with the type a browser
    /// needs to see for each.
    const EXPECTED_TYPES: [(&str, &str); 6] = [
        ("mp3", "audio/mpeg"),
        ("m4a", "audio/"),
        ("aac", "audio/"),
        ("wav", "audio/"),
        ("aiff", "audio/"),
        ("flac", "audio/"),
    ];

    fn catalog_with_file(extension: &str) -> (tempfile::TempDir, AudioCatalog) {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join(format!("track.{extension}"));
        std::fs::write(&file, FILE_BYTES).expect("write audio file");
        (dir, AudioCatalog::new([(KNOWN_TRACK_ID, file)]))
    }

    async fn get_audio(
        catalog: AudioCatalog,
        track_id: &str,
        range: Option<&str>,
    ) -> Response<Body> {
        let mut request = axum::http::Request::get(format!("/audio/{track_id}"));
        if let Some(range) = range {
            request = request.header(RANGE, range);
        }
        audio_router(catalog)
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap()
    }

    async fn body_bytes(response: Response<Body>) -> Vec<u8> {
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec()
    }

    #[tokio::test]
    async fn a_cataloged_file_is_served_whole() {
        let (_dir, catalog) = catalog_with_file("mp3");

        let response = get_audio(catalog, "42", None).await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(body_bytes(response).await, FILE_BYTES);
    }

    #[tokio::test]
    async fn a_range_request_gets_partial_content() {
        let (_dir, catalog) = catalog_with_file("mp3");

        let response = get_audio(catalog, "42", Some("bytes=2-4")).await;

        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(response.headers()[CONTENT_RANGE], "bytes 2-4/10");
        assert_eq!(body_bytes(response).await, b"234");
    }

    #[tokio::test]
    async fn an_unknown_or_malformed_id_is_not_found() {
        let (_dir, catalog) = catalog_with_file("mp3");

        let unknown = get_audio(catalog.clone(), "7", None).await;
        assert_eq!(unknown.status(), StatusCode::NOT_FOUND);

        let traversal = get_audio(catalog, "..%2F..%2Fsecret", None).await;
        assert!(traversal.status().is_client_error());
    }

    #[tokio::test]
    async fn every_audio_extension_gets_an_audio_content_type() {
        for (extension, expected_prefix) in EXPECTED_TYPES {
            let (_dir, catalog) = catalog_with_file(extension);

            let response = get_audio(catalog, "42", None).await;

            let content_type = response.headers()[CONTENT_TYPE].to_str().unwrap();
            assert!(
                content_type.starts_with(expected_prefix),
                ".{extension} was served as {content_type}"
            );
        }
    }

    #[test]
    fn the_url_exists_only_for_a_cataloged_track() {
        let catalog = AudioCatalog::new([(5, PathBuf::from("a.mp3"))]);

        assert_eq!(catalog.url_for(5).as_deref(), Some("/audio/5"));
        assert_eq!(catalog.url_for(6), None);
        assert_eq!(catalog.len(), 1);
    }
}
