//! End-to-end coverage of the control server: a real socket against a
//! manual rig.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use decktector_core::sim::{SimEngine, SimLibrary};
use decktector_core::Engine;
use decktector_remote::AssetSource;
use decktector_sim_control::{AudioCatalog, SimControlServer};
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_tungstenite::tungstenite::Message;

const TIMEOUT: Duration = Duration::from_secs(5);
const SHORT_GRACE: Duration = Duration::from_millis(100);
const DECK_COUNT: usize = 4;
/// Port the (unused) asset proxy points at.
const UNUSED_DEV_SERVER_PORT: u16 = 1;

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn start_server() -> (Arc<SimControlServer>, SimEngine) {
    let (engine, control) = SimEngine::manual(SimLibrary::generated(), None);
    engine.start().await.expect("the manual engine starts");
    let server = SimControlServer::start_on(
        SocketAddr::from(([127, 0, 0, 1], 0)),
        control,
        AssetSource::DevServer(UNUSED_DEV_SERVER_PORT),
        AudioCatalog::default(),
    )
    .await
    .expect("binding an ephemeral port succeeds");
    (server, engine)
}

async fn connect(server: &SimControlServer) -> Socket {
    let url = server
        .page_url()
        .replace("http://", "ws://")
        .replace("/sim-cdj.html", "/ws");
    let (socket, _response) = tokio::time::timeout(TIMEOUT, tokio_tungstenite::connect_async(url))
        .await
        .expect("connecting did not hang")
        .expect("the server accepted the connection");
    socket
}

async fn next_json(socket: &mut Socket) -> serde_json::Value {
    loop {
        let frame = tokio::time::timeout(TIMEOUT, socket.next())
            .await
            .expect("a frame arrived before the timeout")
            .expect("the socket stayed open")
            .expect("the frame was not a protocol error");
        if let Message::Text(text) = frame {
            return serde_json::from_str(&text).expect("the frame is JSON");
        }
    }
}

#[tokio::test]
async fn a_client_gets_the_state_and_every_deck_then_can_play_a_deck() {
    let (server, _engine) = start_server().await;
    let mut socket = connect(&server).await;

    let mut track_messages = 0;
    let mut state_messages = 0;
    while track_messages < DECK_COUNT || state_messages == 0 {
        let message = next_json(&mut socket).await;
        match message["type"].as_str() {
            Some("track") => track_messages += 1,
            Some("state") => state_messages += 1,
            other => panic!("unexpected message type {other:?}"),
        }
    }
    assert_eq!(track_messages, DECK_COUNT);

    // Malformed input must not close the connection.
    socket
        .send(Message::Text("not json".into()))
        .await
        .expect("the malformed frame is sent");
    socket
        .send(Message::Text(r#"{"type":"playPause","deck":1}"#.into()))
        .await
        .expect("the command is sent");

    let deadline = tokio::time::Instant::now() + TIMEOUT;
    loop {
        assert!(
            tokio::time::Instant::now() < deadline,
            "deck 1 never started playing"
        );
        let message = next_json(&mut socket).await;
        if message["type"] == "state" && message["state"]["decks"][0]["playState"] == "playing" {
            break;
        }
    }
}

#[tokio::test]
async fn wait_for_client_sees_a_connected_page_and_times_out_without_one() {
    let (server, _engine) = start_server().await;
    assert!(!server.wait_for_client(SHORT_GRACE).await);

    let _socket = connect(&server).await;
    assert!(server.wait_for_client(TIMEOUT).await);
}

/// Starts a server whose catalog holds `audio_file` for library track 0,
/// returning that track's id alongside.
async fn start_server_with_audio(
    audio_file: &std::path::Path,
) -> (Arc<SimControlServer>, SimEngine, u32) {
    let (engine, control) = SimEngine::manual(SimLibrary::generated(), None);
    engine.start().await.expect("the manual engine starts");
    let track_id = control
        .track(0)
        .expect("the library has a first track")
        .track_id;
    let server = SimControlServer::start_on(
        SocketAddr::from(([127, 0, 0, 1], 0)),
        control,
        AssetSource::DevServer(UNUSED_DEV_SERVER_PORT),
        AudioCatalog::new([(track_id, audio_file.to_path_buf())]),
    )
    .await
    .expect("binding an ephemeral port succeeds");
    (server, engine, track_id)
}

/// One `GET` over a plain TCP connection; returns the raw response text.
async fn http_get(server: &SimControlServer, path: &str) -> String {
    let host = server
        .page_url()
        .trim_start_matches("http://")
        .split('/')
        .next()
        .expect("the page URL has a host")
        .to_string();
    let mut stream = tokio::net::TcpStream::connect(&host)
        .await
        .expect("the server accepts the connection");
    let request = format!(
        "GET {path} HTTP/1.1
Host: {host}
Connection: close

"
    );
    stream
        .write_all(request.as_bytes())
        .await
        .expect("request sent");
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .await
        .expect("response read");
    String::from_utf8_lossy(&response).into_owned()
}

#[tokio::test]
async fn a_loaded_track_with_audio_announces_its_url_and_the_url_serves_the_file() {
    let dir = tempfile::tempdir().expect("temp dir");
    let audio_file = dir.path().join("track.mp3");
    std::fs::write(&audio_file, b"mp3 bytes").expect("write audio file");
    let (server, _engine, track_id) = start_server_with_audio(&audio_file).await;
    let mut socket = connect(&server).await;

    socket
        .send(Message::Text(
            r#"{"type":"load","deck":1,"trackIndex":0}"#.into(),
        ))
        .await
        .expect("the load command is sent");

    let expected_url = format!("/audio/{track_id}");
    let deadline = tokio::time::Instant::now() + TIMEOUT;
    loop {
        assert!(
            tokio::time::Instant::now() < deadline,
            "deck 1 never announced its audio"
        );
        let message = next_json(&mut socket).await;
        if message["type"] == "track" && message["deckId"] == 1 && !message["track"].is_null() {
            assert_eq!(message["audioUrl"], expected_url.as_str());
            break;
        }
    }

    let response = http_get(&server, &expected_url).await;
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(response.contains("audio/mpeg"), "{response}");
    assert!(response.ends_with("mp3 bytes"), "{response}");

    let missing = http_get(&server, "/audio/999999999").await;
    assert!(missing.starts_with("HTTP/1.1 404"), "{missing}");
}
