//! End-to-end coverage of the remote WebSocket server: a real TCP
//! connection, through the token gate, onto the wire format.

use std::sync::Arc;
use std::time::Duration;

use decktective_core::sim::SimEngine;
use decktective_core::Engine;
use decktective_remote::server::{RemoteConfig, RemoteService};
use decktective_remote::AssetSource;
use decktective_remote::wire::{RemoteMessage, SNAPSHOT_CHANNEL};
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
    assert!(status.listening, "service should report itself listening after a successful bind");
    (service, status.port)
}

#[tokio::test]
async fn connecting_with_the_current_token_yields_a_snapshot_first() {
    let (service, port) = start_test_service().await;
    let token = service.status().token.expect("a listening service has a token");

    let url = format!("ws://127.0.0.1:{port}/ws?token={token}");
    let (mut socket, _response) = tokio::time::timeout(Duration::from_secs(5), tokio_tungstenite::connect_async(url))
        .await
        .expect("connection attempt did not hang")
        .expect("the server accepted the connection");

    let first_frame = tokio::time::timeout(Duration::from_secs(5), futures_util::StreamExt::next(&mut socket))
        .await
        .expect("a first frame arrived before the timeout")
        .expect("the socket did not close before sending anything")
        .expect("the frame was not a protocol error");

    let text = match first_frame {
        Message::Text(text) => text,
        other => panic!("expected a text frame, got {other:?}"),
    };
    let message: RemoteMessage = serde_json::from_str(&text).expect("the frame is a valid RemoteMessage");
    assert_eq!(message.channel, SNAPSHOT_CHANNEL);
}

#[tokio::test]
async fn connecting_without_a_token_is_rejected() {
    let (_service, port) = start_test_service().await;

    let url = format!("ws://127.0.0.1:{port}/ws");
    let result = tokio::time::timeout(Duration::from_secs(5), tokio_tungstenite::connect_async(url))
        .await
        .expect("connection attempt did not hang");

    assert!(result.is_err(), "a missing token must not be allowed to upgrade to a WebSocket");
}

#[tokio::test]
async fn connecting_with_the_wrong_token_is_rejected() {
    let (_service, port) = start_test_service().await;

    let url = format!("ws://127.0.0.1:{port}/ws?token=not-the-real-token");
    let result = tokio::time::timeout(Duration::from_secs(5), tokio_tungstenite::connect_async(url))
        .await
        .expect("connection attempt did not hang");

    assert!(result.is_err(), "a wrong token must not be allowed to upgrade to a WebSocket");
}
