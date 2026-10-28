//! Exercises `prolink::dbserver::Connection` against a real in-process TCP
//! server — no mocks — the same "real sockets" philosophy as
//! `crates/prolink-engine/tests/live_against_cdj_sim.rs`. Runs in normal
//! `cargo test`: unlike that UDP-broadcast test, this is loopback TCP on an
//! ephemeral port, so it needs no `#[ignore]` gate for sandbox/CI
//! compatibility.
//!
//! Connects via [`Connection::connect`] (the port-already-known entry
//! point) rather than [`Connection::open`], so these tests never touch the
//! real, fixed port-discovery port (12523) — binding that in a test would
//! risk colliding with anything else on the machine already using it.

use std::net::Ipv4Addr;
use std::time::Duration;

use prolink::dbserver::message::{Field, Message};
use prolink::dbserver::{beatgrid, cues, queries, waveform, Connection};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;

const LOOPBACK: Ipv4Addr = Ipv4Addr::LOCALHOST;
const OUR_DEVICE_NUMBER: u8 = 5;
const TARGET_DEVICE_NUMBER: u8 = 2;
const SLOT: u8 = 0x03; // USB
const TRACK_TYPE: u8 = 0x01; // Rekordbox-analyzed
const TRACK_ID: u32 = 4207;

async fn start_listener() -> (TcpListener, u16) {
    let listener = TcpListener::bind((LOOPBACK, 0))
        .await
        .expect("bind an ephemeral port");
    let port = listener.local_addr().expect("local addr").port();
    (listener, port)
}

async fn read_message(stream: &mut TcpStream, buffer: &mut Vec<u8>) -> Message {
    loop {
        if let Some((message, consumed)) = Message::decode(buffer) {
            buffer.drain(..consumed);
            return message;
        }
        let mut chunk = [0u8; 4096];
        let read = stream.read(&mut chunk).await.expect("read from the client");
        assert_ne!(
            read, 0,
            "client closed the connection before sending a complete message"
        );
        buffer.extend_from_slice(&chunk[..read]);
    }
}

/// Answers the bare number field a real player expects before any message
/// — see `Connection::greet`. Every fake server here has to do it, or the
/// client under test blocks waiting for the echo, exactly as it would
/// against hardware that refused the connection.
async fn greet(stream: &mut TcpStream) {
    let mut greeting = [0u8; 5];
    stream
        .read_exact(&mut greeting)
        .await
        .expect("read the client's greeting");
    stream
        .write_all(&greeting)
        .await
        .expect("echo the greeting");
}

/// What a player answers a metadata request with: the request type echoed
/// back, then how many menu items it is willing to render.
fn menu_available_args(item_count: u32) -> Vec<Field> {
    vec![
        Field::U32(u32::from(queries::METADATA_REQ)),
        Field::U32(item_count),
    ]
}

async fn send(stream: &mut TcpStream, transaction_id: u32, message_type: u16, args: Vec<Field>) {
    let bytes = Message::new(transaction_id, message_type, args).encode();
    stream.write_all(&bytes).await.expect("write to the client");
}

/// A menu item shaped like the ones a CDJ-3000 sends: value at index 1,
/// label length at 2, label at 3, item type at 6.
fn menu_item_args(item_type: u32, text: &str) -> Vec<Field> {
    vec![
        Field::U32(1),                           // parent id
        Field::U32(1),                           // the item's value
        Field::U32(text.chars().count() as u32), // label length
        Field::Str(text.to_string()),
        Field::U32(2),
        Field::Str(String::new()),
        Field::U32(item_type),
        Field::U32(0),
    ]
}

#[tokio::test]
async fn fetches_full_track_metadata_via_the_menu_render_exchange() {
    let (listener, port) = start_listener().await;

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept");
        greet(&mut stream).await;
        let mut buffer = Vec::new();

        let setup = read_message(&mut stream, &mut buffer).await;
        assert_eq!(setup.message_type, queries::SETUP_REQ);
        send(&mut stream, 0, queries::SETUP_REQ, vec![]).await;

        let metadata_req = read_message(&mut stream, &mut buffer).await;
        assert_eq!(metadata_req.message_type, queries::METADATA_REQ);
        send(
            &mut stream,
            1,
            queries::MENU_AVAILABLE,
            menu_available_args(2),
        )
        .await;

        let render_req = read_message(&mut stream, &mut buffer).await;
        assert_eq!(render_req.message_type, queries::RENDER_MENU_REQ);
        send(&mut stream, 2, queries::MENU_HEADER, vec![]).await;
        send(
            &mut stream,
            3,
            queries::MENU_ITEM,
            menu_item_args(0x0004, "Strobe"),
        )
        .await;
        send(
            &mut stream,
            4,
            queries::MENU_ITEM,
            menu_item_args(0x0007, "deadmau5"),
        )
        .await;
        send(&mut stream, 5, queries::MENU_FOOTER, vec![]).await;
    });

    let mut connection = Connection::connect(LOOPBACK, LOOPBACK, port, OUR_DEVICE_NUMBER)
        .await
        .expect("should connect");
    let items = connection
        .fetch_metadata(TARGET_DEVICE_NUMBER, SLOT, TRACK_TYPE, TRACK_ID)
        .await
        .expect("should fetch metadata");

    assert_eq!(items.len(), 2);
    assert_eq!(items[0].text.as_deref(), Some("Strobe"));
    assert_eq!(items[1].text.as_deref(), Some("deadmau5"));

    server.await.expect("server task should finish cleanly");
}

#[tokio::test]
async fn a_zero_item_count_yields_an_empty_metadata_list_without_a_render_step() {
    let (listener, port) = start_listener().await;

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept");
        greet(&mut stream).await;
        let mut buffer = Vec::new();
        read_message(&mut stream, &mut buffer).await; // SETUP_REQ
        send(&mut stream, 0, queries::SETUP_REQ, vec![]).await;
        read_message(&mut stream, &mut buffer).await; // METADATA_REQ
        send(
            &mut stream,
            1,
            queries::MENU_AVAILABLE,
            menu_available_args(0),
        )
        .await;
    });

    let mut connection = Connection::connect(LOOPBACK, LOOPBACK, port, OUR_DEVICE_NUMBER)
        .await
        .expect("should connect");
    let items = connection
        .fetch_metadata(TARGET_DEVICE_NUMBER, SLOT, TRACK_TYPE, TRACK_ID)
        .await
        .expect("should fetch");
    assert!(items.is_empty());

    server.await.expect("server task should finish cleanly");
}

/// An unanalysed file off a USB stick: asked about with the rekordbox
/// metadata request, a player renders an empty menu and the deck shows
/// nothing at all.
#[tokio::test]
async fn an_unanalysed_track_is_asked_about_with_the_unanalyzed_metadata_request() {
    const UNANALYSED_FILE: u8 = 0x02;
    let (listener, port) = start_listener().await;

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept");
        greet(&mut stream).await;
        let mut buffer = Vec::new();
        read_message(&mut stream, &mut buffer).await; // SETUP_REQ
        send(&mut stream, 0, queries::SETUP_REQ, vec![]).await;

        let metadata_req = read_message(&mut stream, &mut buffer).await;
        send(
            &mut stream,
            1,
            queries::MENU_AVAILABLE,
            menu_available_args(0),
        )
        .await;
        metadata_req.message_type
    });

    let mut connection = Connection::connect(LOOPBACK, LOOPBACK, port, OUR_DEVICE_NUMBER)
        .await
        .expect("should connect");
    connection
        .fetch_metadata(TARGET_DEVICE_NUMBER, SLOT, UNANALYSED_FILE, TRACK_ID)
        .await
        .expect("should fetch");

    let request_type = server.await.expect("server task should finish cleanly");
    assert_eq!(request_type, queries::UNANALYZED_METADATA_REQ);
}

#[tokio::test]
async fn fetches_and_parses_a_beat_grid_blob() {
    let (listener, port) = start_listener().await;

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept");
        greet(&mut stream).await;
        let mut buffer = Vec::new();
        read_message(&mut stream, &mut buffer).await; // SETUP_REQ
        send(&mut stream, 0, queries::SETUP_REQ, vec![]).await;

        read_message(&mut stream, &mut buffer).await; // BEAT_GRID_REQ
        let mut blob = vec![0u8; 20]; // Header this parser skips.
                                      // One entry: beat 1, 128.00 BPM, at 0ms (little-endian, per beatgrid.rs).
        blob.extend_from_slice(&1u16.to_le_bytes());
        blob.extend_from_slice(&12_800u16.to_le_bytes());
        blob.extend_from_slice(&0u32.to_le_bytes());
        blob.extend_from_slice(&[0u8; 8]);
        send(
            &mut stream,
            1,
            queries::BEAT_GRID_RESPONSE,
            vec![Field::Blob(blob)],
        )
        .await;
    });

    let mut connection = Connection::connect(LOOPBACK, LOOPBACK, port, OUR_DEVICE_NUMBER)
        .await
        .expect("should connect");
    let blob = connection
        .fetch_beat_grid(TARGET_DEVICE_NUMBER, SLOT, TRACK_TYPE, TRACK_ID)
        .await
        .expect("should fetch")
        .expect("should have a blob");

    let entries = beatgrid::parse_beat_grid(&blob);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].beat_in_bar, 1);
    assert_eq!(entries[0].tempo_centibpm, 12_800);

    server.await.expect("server task should finish cleanly");
}

#[tokio::test]
async fn fetches_and_parses_a_waveform_blob() {
    let (listener, port) = start_listener().await;

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept");
        greet(&mut stream).await;
        let mut buffer = Vec::new();
        read_message(&mut stream, &mut buffer).await; // SETUP_REQ
        send(&mut stream, 0, queries::SETUP_REQ, vec![]).await;

        let request = read_message(&mut stream, &mut buffer).await;
        let mut blob = vec![0u8; 28]; // Leading junk this parser skips.
                                      // One full-height white column.
        let bits: u16 = (31u16 << 2) | (7u16 << 13) | (7u16 << 10) | (7u16 << 7);
        blob.extend_from_slice(&bits.to_be_bytes());
        send(
            &mut stream,
            1,
            queries::ANLZ_TAG_RESPONSE,
            vec![Field::Blob(blob)],
        )
        .await;
        request
    });

    let mut connection = Connection::connect(LOOPBACK, LOOPBACK, port, OUR_DEVICE_NUMBER)
        .await
        .expect("should connect");
    let blob = connection
        .fetch_anlz_tag(
            TARGET_DEVICE_NUMBER,
            SLOT,
            TRACK_TYPE,
            TRACK_ID,
            waveform::COLOR_WAVEFORM_DETAIL_TAG,
            waveform::COLOR_WAVEFORM_FILE_EXTENSION,
        )
        .await
        .expect("should fetch")
        .expect("should have a blob");

    let columns = waveform::parse_color_waveform_detail(&blob);
    assert_eq!(columns.len(), 1);
    assert_eq!(columns[0].height, 1.0);
    assert_eq!(columns[0].color, [255, 255, 255]);

    // Every argument a number field, in `beat-link`'s order. The file
    // extension used to go out as the text ".EXT", a request no player
    // recognises.
    let request = server.await.expect("server task should finish cleanly");
    assert_eq!(request.message_type, queries::ANLZ_TAG_REQ);
    assert_eq!(
        request.arguments,
        vec![
            Field::U32(u32::from_be_bytes([
                TARGET_DEVICE_NUMBER,
                queries::MENU_TARGET_MAIN,
                SLOT,
                TRACK_TYPE,
            ])),
            Field::U32(TRACK_ID),
            Field::U32(0x3556_5750), // "PWV5", byte-reversed
            Field::U32(0x0054_5845), // "EXT", byte-reversed, NUL-padded
        ]
    );
}

fn decode_hex(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("valid hex"))
        .collect()
}

/// What two CDJ-2000NXSs answered real decks' cue list requests with
/// (`usr-ein/prolink` capture corpus), byte for byte: a track with hot cues
/// A-C (`S06-load-and-play`), then a track with none, whose reply leaves
/// both empty blobs out (`S13-format-ground-truth`).
const CUE_LIST_WITH_THREE_HOT_CUES: &str = concat!(
    "11872349ae110380036f1047020f09140000000c06060603060606060300000011000021",
    "041100000000110000006c140000006c0001010000000000000000002800000000000000",
    "000000000000000000000000000000000001020000000000000000007125000000000000",
    "000000000000000000000000000000000001030000000000000000005b34000000000000",
    "000000000000000000000000000000001100000024110000000311000000001100000018",
    "14000000180f010000ffffffff9ef90000ffffffff0b5d0100ffffffff",
);
const CUE_LIST_WITHOUT_CUES: &str = concat!(
    "11872349ae11038000131047020f09140000000c060606030606060603000000",
    "1100002104110000000011000000001100000024",
    "110000000011000000001100000000",
);

#[tokio::test]
async fn reads_a_cdj_2000nxss_cue_lists_without_waiting_on_an_empty_one() {
    let (listener, port) = start_listener().await;

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept");
        greet(&mut stream).await;
        let mut buffer = Vec::new();
        read_message(&mut stream, &mut buffer).await; // SETUP_REQ
        send(&mut stream, 0, queries::SETUP_REQ, vec![]).await;

        let mut requests = Vec::new();
        for reply in [CUE_LIST_WITH_THREE_HOT_CUES, CUE_LIST_WITHOUT_CUES] {
            requests.push(read_message(&mut stream, &mut buffer).await);
            stream
                .write_all(&decode_hex(reply))
                .await
                .expect("write to the client");
        }
        requests
    });

    let mut connection = Connection::connect(LOOPBACK, LOOPBACK, port, OUR_DEVICE_NUMBER)
        .await
        .expect("should connect");
    let with_cues = connection
        .fetch_cues_basic(TARGET_DEVICE_NUMBER, SLOT, TRACK_TYPE, TRACK_ID)
        .await
        .expect("should fetch");
    // Far under the client's five-second quiet timeout: the empty reply is
    // complete as sent, not a message still waiting for two arguments.
    let without_cues = timeout(
        Duration::from_secs(1),
        connection.fetch_cues_basic(TARGET_DEVICE_NUMBER, SLOT, TRACK_TYPE, TRACK_ID),
    )
    .await
    .expect("an empty cue list should not wait for the read timeout")
    .expect("should fetch");

    let hot_cues: Vec<_> = cues::parse_cue_points(&with_cues)
        .iter()
        .map(|cue| (cue.hot_cue, cue.time_ms))
        .collect();
    assert_eq!(
        hot_cues,
        vec![(Some(1), 271), (Some(2), 63_902), (Some(3), 89_355)]
    );
    assert!(cues::parse_cue_points(&without_cues).is_empty());

    let requests = server.await.expect("server task should finish cleanly");
    assert!(requests
        .iter()
        .all(|request| request.message_type == queries::CUE_LIST_REQ));
}

/// The `b5cbf4f`-class regression, at the transport level: a server that
/// accepts the connection and never answers — not even the setup handshake
/// — must not hang the client forever; a caller-applied timeout has to be
/// able to interrupt it at any stage, including [`Connection::connect`]
/// itself. (The orchestration-level half of this regression — that one
/// stalled device doesn't block metadata fetches for a *different* device,
/// and that a failed device is skipped for a cooldown period rather than
/// retried on every track change — is covered by `prolink-engine`'s own
/// `MetadataFetcher` unit tests, since it isn't reachable from this crate.)
#[tokio::test]
async fn a_connection_that_never_answers_can_be_interrupted_by_a_timeout() {
    let (listener, port) = start_listener().await;

    let server = tokio::spawn(async move {
        let (_stream, _) = listener.accept().await.expect("accept");
        // Accept the connection, read nothing, answer nothing, just hold it open.
        tokio::time::sleep(Duration::from_secs(5)).await;
    });

    // The hang would happen here — inside the setup handshake `connect`
    // performs — not in a later query, so the timeout has to wrap this
    // call itself, not just a query made on an already-open connection.
    let result = timeout(
        Duration::from_millis(200),
        Connection::connect(LOOPBACK, LOOPBACK, port, OUR_DEVICE_NUMBER),
    )
    .await;

    assert!(
        result.is_err(),
        "connect should have been interrupted by the timeout, not hung or errored on its own"
    );
    server.abort();
}
