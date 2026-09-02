//! End-to-end coverage of the remote WebSocket server: a real TCP
//! connection, through the token gate, onto the wire format.

use std::sync::Arc;
use std::time::Duration;

use decktective_core::sim::SimEngine;
use decktective_core::Engine;
use decktective_remote::server::{RemoteConfig, RemoteService};
use decktective_remote::wire::{RemoteMessage, SNAPSHOT_CHANNEL};
use decktective_remote::AssetSource;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;

/// Number of simulated decks the test engine reports — arbitrary, just
/// needs to be a valid `SimEngine` configuration.
const TEST_DECK_COUNT: u8 = 2;

async fn start_test_service() -> (Arc<RemoteService>, u16) {
    let engine: Arc<dyn Engine> = Arc::new(SimEngine::new(TEST_DECK_COUNT));
    let service = RemoteService::new(engine, AssetSource::DevServer(1420));

    service
        .clone()
        .set_config(RemoteConfig {
            enabled: true,
            port: 0,
            interface: String::new(),
        })
        .await
        .expect("set_config does not fail on a successful bind");

    let status = service.status();
    assert!(
        status.listening,
        "service should report itself listening after a successful bind"
    );
    (service, status.port)
}

#[tokio::test]
async fn connecting_with_the_current_token_yields_a_snapshot_first() {
    let (service, port) = start_test_service().await;
    let token = service
        .status()
        .token
        .expect("a listening service has a token");

    let url = format!("ws://127.0.0.1:{port}/ws?token={token}");
    let (mut socket, _response) = tokio::time::timeout(
        Duration::from_secs(5),
        tokio_tungstenite::connect_async(url),
    )
    .await
    .expect("connection attempt did not hang")
    .expect("the server accepted the connection");

    let first_frame = tokio::time::timeout(
        Duration::from_secs(5),
        futures_util::StreamExt::next(&mut socket),
    )
    .await
    .expect("a first frame arrived before the timeout")
    .expect("the socket did not close before sending anything")
    .expect("the frame was not a protocol error");

    let text = match first_frame {
        Message::Text(text) => text,
        other => panic!("expected a text frame, got {other:?}"),
    };
    let message: RemoteMessage =
        serde_json::from_str(&text).expect("the frame is a valid RemoteMessage");
    assert_eq!(message.channel, SNAPSHOT_CHANNEL);
}

#[tokio::test]
async fn connecting_without_a_token_is_rejected() {
    let (_service, port) = start_test_service().await;

    let url = format!("ws://127.0.0.1:{port}/ws");
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        tokio_tungstenite::connect_async(url),
    )
    .await
    .expect("connection attempt did not hang");

    assert!(
        result.is_err(),
        "a missing token must not be allowed to upgrade to a WebSocket"
    );
}

#[tokio::test]
async fn connecting_with_the_wrong_token_is_rejected() {
    let (_service, port) = start_test_service().await;

    let url = format!("ws://127.0.0.1:{port}/ws?token=not-the-real-token");
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        tokio_tungstenite::connect_async(url),
    )
    .await
    .expect("connection attempt did not hang");

    assert!(
        result.is_err(),
        "a wrong token must not be allowed to upgrade to a WebSocket"
    );
}

/// A minimal HTTP/1.1 GET over a raw TCP connection, just enough to read a
/// status line back — no need to pull in a full HTTP client crate for two
/// assertions.
async fn get_status_line(port: u16, path_and_query: &str) -> String {
    let mut stream = tokio::time::timeout(
        Duration::from_secs(5),
        TcpStream::connect(("127.0.0.1", port)),
    )
    .await
    .expect("connect attempt did not hang")
    .expect("connects to the listening server");

    let request = format!(
        "GET {path_and_query} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .await
        .expect("request writes");

    let mut response = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), stream.read_to_end(&mut response))
        .await
        .expect("response arrives before the timeout")
        .expect("reading the response does not error");

    let text = String::from_utf8_lossy(&response);
    text.lines()
        .next()
        .expect("response has a status line")
        .to_string()
}

#[tokio::test]
async fn track_endpoints_require_the_token_like_every_other_remote_route() {
    let (_service, port) = start_test_service().await;

    let status = get_status_line(port, "/track/1/artwork").await;

    assert!(
        status.contains("401"),
        "expected a 401 without a token, got: {status}"
    );
}

#[tokio::test]
async fn an_unknown_tracks_artwork_and_waveform_are_not_found() {
    let (service, port) = start_test_service().await;
    let token = service
        .status()
        .token
        .expect("a listening service has a token");

    let artwork_status =
        get_status_line(port, &format!("/track/999999/artwork?token={token}")).await;
    let waveform_status =
        get_status_line(port, &format!("/track/999999/waveform?token={token}")).await;

    assert!(
        artwork_status.contains("404"),
        "expected a 404, got: {artwork_status}"
    );
    assert!(
        waveform_status.contains("404"),
        "expected a 404, got: {waveform_status}"
    );
}
