//! Validates the `prolink-capture` binary itself (see
//! `HARDWARE-CAPTURE-PLAN.md`, repo root) against this crate's existing
//! simulators before ever pointing it at real hardware:
//!
//! - phases A/B against `cdj-sim`, the same broadcast simulator
//!   `prolink-engine` already exercises against;
//! - phases C/D against small fake NFS/dbserver servers built from this
//!   crate's own wire-format encoders (the same building blocks
//!   `tests/nfs_fake_server.rs`/`tests/dbserver_fake_server.rs` use);
//! - three "deliberately break things" scenarios: an unreachable player, a
//!   truncated `export.pdb`, and killing the tool mid-capture — in every
//!   case the session directory must still hold every byte captured up to
//!   that point.
//!
//! The NFS half binds the real, fixed portmapper port (111) — the
//! `prolink-capture` binary always calls the real `NfsClient::open`, which
//! (unlike this crate's own fake-server tests) has no test-only alternate
//! -port entry point, since the binary isn't in a position to be told about
//! one. Unlike Unix, Windows has no privileged-port restriction below 1024
//! for an ordinary user, so this is expected to work in the same
//! environment this tool ships for; if it can't bind (already in use, or a
//! sandbox that does restrict it), that phase of the test logs and skips
//! rather than failing the whole suite.

mod support;

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use prolink::capture::{read_jsonl, UdpRecord};
use prolink::dbserver::message::{Field, Message};
use prolink::dbserver::queries;
use prolink::nfs::mount::FileHandle;
use prolink::nfs::procedures::FileAttributes;
use prolink::nfs::rpc::{self, RpcCall};
use prolink::nfs::xdr::XdrDecoder;
use prolink::nfs::{mount, portmap, procedures};
use prolink::status::{build_status, RawPlayState};
use prolink::{nfs::pdb, ANNOUNCE_PORT, BEAT_PORT, STATUS_PORT};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::process::{Child, Command};
use tokio::time::timeout;

/// Every test in this file that spawns `prolink-capture` binds fixed,
/// well-known ports (50000-50002, 111, 12523) — real OS-wide resources, not
/// scoped per test. Rust's default test harness runs `#[tokio::test]`
/// functions concurrently on separate threads, and unicast delivery to a
/// port with more than one bound socket (even under `SO_REUSEADDR`) is not
/// guaranteed to go to the "right" one — observed directly as cross-test
/// packet delivery when this lock was absent. Every such test acquires this
/// for its whole body instead, serializing them.
static SHARED_NETWORK_PORTS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn capture_binary() -> &'static str {
    env!("CARGO_BIN_EXE_prolink-capture")
}

fn cdj_sim_binary() -> &'static str {
    env!("CARGO_BIN_EXE_cdj-sim")
}

fn spawn_capture(out_dir: &Path, extra_args: &[&str]) -> Child {
    Command::new(capture_binary())
        .arg("--out")
        .arg(out_dir)
        .args(extra_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn prolink-capture")
}

/// The one `capture-*` directory a run produced.
fn session_dir(out_dir: &Path) -> PathBuf {
    std::fs::read_dir(out_dir)
        .unwrap_or_else(|error| panic!("read {}: {error}", out_dir.display()))
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .find(|path| {
            path.is_dir()
                && path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("capture-"))
        })
        .unwrap_or_else(|| panic!("no capture-* session directory under {}", out_dir.display()))
}

// ---------------------------------------------------------------------
// cdj-sim replay
// ---------------------------------------------------------------------

/// Replay is what lets a recorded evening drive the real parsing path, so
/// the two things it promises are pinned here: the bytes come back out
/// unchanged, and they come back out with the spacing they were recorded
/// with. A replay that fired everything at once would look like a working
/// network and behave like nothing that has ever happened in a booth.
#[tokio::test]
async fn cdj_sim_replays_a_recorded_session_with_its_original_timing() {
    let _ports = SHARED_NETWORK_PORTS.lock().await;
    let session = tempfile::tempdir().expect("temp session dir");

    let first = build_status(
        1,
        RawPlayState::Playing,
        true,
        false,
        true,
        0.0,
        Some(128.0),
        Some(64),
        Some(1),
        Some(100),
    );
    let second = build_status(
        2,
        RawPlayState::Playing,
        false,
        true,
        false,
        0.0,
        Some(130.0),
        Some(32),
        Some(3),
        Some(200),
    );
    // 400 ms apart, at timestamps far from now: replay must anchor on the
    // first record's own time, not on the wall clock it happens to run at.
    let recorded_at = 1_700_000_000_000_000i64;
    let records = [
        UdpRecord::at(
            recorded_at,
            STATUS_PORT,
            IpAddr::V4(Ipv4Addr::new(169, 254, 7, 50)),
            &first,
        ),
        UdpRecord::at(
            recorded_at + 400_000,
            STATUS_PORT,
            IpAddr::V4(Ipv4Addr::new(169, 254, 6, 92)),
            &second,
        ),
    ];
    let lines: Vec<String> = records
        .iter()
        .map(|record| serde_json::to_string(record).expect("serialise"))
        .collect();
    std::fs::write(session.path().join("udp.jsonl"), lines.join("\n")).expect("write udp.jsonl");

    let listener = UdpSocket::bind((Ipv4Addr::LOCALHOST, STATUS_PORT))
        .await
        .expect("bind the status port");

    let mut sim = Command::new(cdj_sim_binary())
        .args(["--replay", &session.path().display().to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn cdj-sim");

    let mut received: Vec<(Vec<u8>, Duration)> = Vec::new();
    let started_at = std::time::Instant::now();
    for _ in 0..records.len() {
        let mut buf = vec![0u8; 2048];
        let (len, _) = timeout(Duration::from_secs(10), listener.recv_from(&mut buf))
            .await
            .expect("a replayed datagram should arrive within 10s")
            .expect("receive");
        buf.truncate(len);
        received.push((buf, started_at.elapsed()));
    }
    let _ = sim.kill().await;

    assert_eq!(
        received[0].0, first,
        "the first datagram should be replayed byte for byte"
    );
    assert_eq!(
        received[1].0, second,
        "the second datagram should be replayed byte for byte"
    );

    let gap = received[1].1.saturating_sub(received[0].1);
    assert!(
        gap >= Duration::from_millis(250),
        "the recorded 400ms gap should be preserved, got {gap:?}"
    );
}

// ---------------------------------------------------------------------
// Phases A/B against cdj-sim
// ---------------------------------------------------------------------

#[tokio::test]
async fn phases_a_and_b_capture_every_simulated_device_at_a_full_decode_rate() {
    let _ports = SHARED_NETWORK_PORTS.lock().await;
    let out_dir = tempfile::tempdir().expect("temp out dir");

    let mut sim = Command::new(cdj_sim_binary())
        .args(["--count", "4", "--mixer", "--interval-ms", "300"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn cdj-sim");
    tokio::time::sleep(Duration::from_millis(300)).await; // Let it start broadcasting before capture binds.

    let mut capture = spawn_capture(
        out_dir.path(),
        &["--duration-sec", "4", "--no-nfs", "--no-dbserver"],
    );
    let status = timeout(Duration::from_secs(30), capture.wait())
        .await
        .expect("prolink-capture should exit on its own within --duration-sec")
        .expect("wait on prolink-capture");
    assert!(status.success(), "prolink-capture exited with {status:?}");
    let _ = sim.kill().await;

    let session = session_dir(out_dir.path());

    let devices_read = read_jsonl::<serde_json::Value>(&session.join("devices.jsonl"))
        .expect("read devices.jsonl");
    let last_snapshot = devices_read
        .records
        .last()
        .expect("at least one device snapshot");
    let devices = last_snapshot["devices"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    // Asserted by membership, not by count: this test binds the real Pro
    // DJ Link ports, so anything else Pro DJ Link on the machine or the
    // LAN — a Decktector window left open, real players in the room —
    // legitimately shows up here too.
    for simulated in [
        "CDJ-3000 #1",
        "CDJ-3000 #2",
        "CDJ-3000 #3",
        "CDJ-3000 #4",
        "DJM-900NXS2",
    ] {
        assert!(
            devices.iter().any(|device| device["name"] == simulated),
            "{simulated} missing from the device table: {last_snapshot}"
        );
    }
    // Our own keep-alive is in there on purpose: without it real players
    // never unicast status packets to us (see `prolink::presence`), so a
    // capture that does not announce itself is a capture with no status
    // traffic in it at all.
    assert!(
        devices.iter().any(|device| device["name"] == "decktector"),
        "prolink-capture should announce itself as a virtual player: {last_snapshot}"
    );

    let udp_read = read_jsonl::<UdpRecord>(&session.join("udp.jsonl")).expect("read udp.jsonl");
    assert!(
        !udp_read.records.is_empty(),
        "udp.jsonl should not be empty"
    );
    for port in [ANNOUNCE_PORT, STATUS_PORT, BEAT_PORT] {
        assert!(
            udp_read.records.iter().any(|record| record.port == port),
            "no udp.jsonl records captured on port {port}"
        );
    }

    let status_read = read_jsonl::<serde_json::Value>(&session.join("status-decoded.jsonl"))
        .expect("read status-decoded.jsonl");
    assert!(
        !status_read.records.is_empty(),
        "status-decoded.jsonl should not be empty"
    );
    let decoded = status_read
        .records
        .iter()
        .filter(|record| record["outcome"] == "decoded")
        .count();
    assert_eq!(decoded, status_read.records.len(), "every status packet from cdj-sim should decode — a real simulator should never produce a decode failure");
}

// ---------------------------------------------------------------------
// Phase C against a fake NFS server
// ---------------------------------------------------------------------

const NFS_ROOT: FileHandle = [0x01; 32];
const NFS_PIONEER: FileHandle = [0x02; 32];
const NFS_REKORDBOX: FileHandle = [0x03; 32];
const NFS_PDB: FileHandle = [0x04; 32];
const NFS_DAT: FileHandle = [0x05; 32];
const NFS_EXT: FileHandle = [0x06; 32];
const PORTMAP_PORT: u16 = 111;

/// Starts a fake NFS server bound to the real portmapper port, serving a
/// synthetic `export.pdb` (one track) plus arbitrary `.DAT`/`.EXT` bytes for
/// that track. `None` if port 111 couldn't be bound in this environment —
/// see this file's module doc comment.
async fn start_fake_nfs_server() -> Option<tokio::task::JoinHandle<()>> {
    let socket = match UdpSocket::bind((Ipv4Addr::LOCALHOST, PORTMAP_PORT)).await {
        Ok(socket) => socket,
        Err(error) => {
            eprintln!("cannot bind UDP port {PORTMAP_PORT} (portmapper) in this environment ({error}); skipping the NFS phase of this test");
            return None;
        }
    };

    let mut payloads = HashMap::new();
    payloads.insert(NFS_PDB, support::build_minimal_pdb_bytes());
    payloads.insert(
        NFS_DAT,
        b"fake beat grid / waveform analysis bytes (.DAT)".to_vec(),
    );
    payloads.insert(NFS_EXT, b"fake extended analysis bytes (.EXT)".to_vec());

    Some(tokio::spawn(async move {
        let mut buf = [0u8; 4096];
        loop {
            let Ok((len, from)) = socket.recv_from(&mut buf).await else {
                break;
            };
            let Some(call) = rpc::parse_call(&buf[..len]) else {
                continue;
            };
            if let Some(reply) = handle_nfs_call(&call, PORTMAP_PORT, &payloads) {
                let _ = socket.send_to(&reply, from).await;
            }
        }
    }))
}

fn handle_nfs_call(
    call: &RpcCall<'_>,
    own_port: u16,
    payloads: &HashMap<FileHandle, Vec<u8>>,
) -> Option<Vec<u8>> {
    match (call.prog, call.proc) {
        (portmap::PMAP_PROGRAM, _) => Some(portmap::build_getport_reply(call.xid, own_port)),
        (mount::MOUNT_PROGRAM, _) => handle_nfs_mount(call),
        (procedures::NFS_PROGRAM, 4) => handle_nfs_lookup(call, payloads),
        (procedures::NFS_PROGRAM, 6) => handle_nfs_read(call, payloads),
        _ => None,
    }
}

fn handle_nfs_mount(call: &RpcCall<'_>) -> Option<Vec<u8>> {
    let mut decoder = XdrDecoder::new(call.args);
    let path = decoder.read_string_utf16le()?;
    (path == "/C/").then(|| mount::build_mnt_reply(call.xid, NFS_ROOT))
}

fn handle_nfs_lookup(
    call: &RpcCall<'_>,
    payloads: &HashMap<FileHandle, Vec<u8>>,
) -> Option<Vec<u8>> {
    let mut decoder = XdrDecoder::new(call.args);
    let dir: FileHandle = decoder.read_opaque_fixed(32)?.try_into().ok()?;
    let name = decoder.read_string_utf16le()?;

    let handle = match (dir, name.as_str()) {
        (NFS_ROOT, "PIONEER") => NFS_PIONEER,
        (NFS_PIONEER, "rekordbox") => NFS_REKORDBOX,
        (NFS_REKORDBOX, "export.pdb") => NFS_PDB,
        (NFS_ROOT, "ANLZ0000.DAT") => NFS_DAT,
        (NFS_ROOT, "ANLZ0000.EXT") => NFS_EXT,
        _ => return None,
    };
    let size = payloads.get(&handle).map_or(0, Vec::len) as u32;
    let attributes = FileAttributes {
        file_type: procedures::FTYPE_REGULAR_FILE,
        size,
    };
    Some(procedures::build_lookup_reply(call.xid, handle, attributes))
}

fn handle_nfs_read(call: &RpcCall<'_>, payloads: &HashMap<FileHandle, Vec<u8>>) -> Option<Vec<u8>> {
    let mut decoder = XdrDecoder::new(call.args);
    let handle: FileHandle = decoder.read_opaque_fixed(32)?.try_into().ok()?;
    let payload = payloads.get(&handle)?;
    let offset = decoder.read_u32()? as usize;
    let count = decoder.read_u32()? as usize;
    let end = (offset + count).min(payload.len());
    let chunk = payload.get(offset..end)?;
    let attributes = FileAttributes {
        file_type: procedures::FTYPE_REGULAR_FILE,
        size: payload.len() as u32,
    };
    Some(procedures::build_read_reply(call.xid, attributes, chunk))
}

#[tokio::test]
async fn phase_c_pulls_export_pdb_and_the_loaded_tracks_analysis_files() {
    let _ports = SHARED_NETWORK_PORTS.lock().await;
    let Some(_server) = start_fake_nfs_server().await else {
        return;
    };
    let out_dir = tempfile::tempdir().expect("temp out dir");

    let mut capture = spawn_capture(
        out_dir.path(),
        &[
            "--bind",
            "127.0.0.1",
            "--player",
            "127.0.0.1",
            "--slot",
            "usb",
            "--duration-sec",
            "1",
            "--no-dbserver",
        ],
    );
    let status = timeout(Duration::from_secs(30), capture.wait())
        .await
        .expect("prolink-capture should exit on its own")
        .expect("wait");
    assert!(status.success(), "prolink-capture exited with {status:?}");

    let session = session_dir(out_dir.path());
    let nfs_dir = session.join("nfs").join("127.0.0.1-usb");

    let pdb_bytes =
        std::fs::read(nfs_dir.join("export.pdb")).expect("export.pdb should have been written");
    assert!(!pdb_bytes.is_empty());
    let parsed = pdb::parse(&pdb_bytes).expect("the captured export.pdb should parse");
    assert_eq!(parsed.tracks.len(), 1);
    let track_id = *parsed.tracks.keys().next().expect("one track");

    let dat_bytes = std::fs::read(nfs_dir.join("analysis").join(format!("{track_id}.DAT")))
        .expect(".DAT should have been written");
    assert!(!dat_bytes.is_empty());
    let ext_bytes = std::fs::read(nfs_dir.join("analysis").join(format!("{track_id}.EXT")))
        .expect(".EXT should have been written");
    assert!(!ext_bytes.is_empty());

    let report: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(nfs_dir.join("parse-report.json")).expect("parse-report.json"),
    )
    .expect("valid json");
    assert_eq!(report["export_pdb"]["parsed"], true);
    assert_eq!(report["export_pdb"]["track_count"], 1);
}

// ---------------------------------------------------------------------
// Phase D against a fake dbserver
// ---------------------------------------------------------------------

const DBSERVER_DEVICE_NUMBER: u8 = 2;
const DBSERVER_TRACK_ID: u32 = 4207;

/// Injects one status packet directly onto loopback so phase D's ledger
/// picks up a "loaded track" for `DBSERVER_DEVICE_NUMBER` without depending
/// on cdj-sim or real broadcast delivery — this crate's own
/// `build_status`/UDP send is a much more deterministic way to get a known
/// device/slot/track-id into the tool's ledger for this specific test.
async fn inject_status_packet() {
    let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("bind an ephemeral sender socket");
    let packet = build_status(
        DBSERVER_DEVICE_NUMBER,
        RawPlayState::Playing,
        true,
        false,
        true,
        0.0,
        Some(128.0),
        Some(16),
        Some(1),
        Some(DBSERVER_TRACK_ID),
    );
    socket
        .send_to(&packet, (Ipv4Addr::LOCALHOST, STATUS_PORT))
        .await
        .expect("send synthetic status packet");
}

/// `None` if the real, fixed port-discovery port (12523) couldn't be bound
/// in this environment — `Connection::open` (used by the real binary)
/// always dials that fixed port, there's no test-only override for it, the
/// same constraint as the NFS portmapper above.
async fn start_fake_dbserver() -> Option<tokio::task::JoinHandle<()>> {
    let query_listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("bind an ephemeral dbserver query port");
    let query_port = query_listener.local_addr().expect("local addr").port();

    let discovery_listener = match TcpListener::bind((
        Ipv4Addr::LOCALHOST,
        prolink::dbserver::client::PORT_DISCOVERY_PORT,
    ))
    .await
    {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("cannot bind TCP port {} (dbserver port discovery) in this environment ({error}); skipping the dbserver phase of this test", prolink::dbserver::client::PORT_DISCOVERY_PORT);
            return None;
        }
    };

    Some(tokio::spawn(async move {
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = discovery_listener.accept().await else {
                    break;
                };
                let mut request = [0u8; 19];
                if stream.read_exact(&mut request).await.is_err() {
                    continue;
                }
                let _ = stream.write_all(&query_port.to_be_bytes()).await;
            }
        });

        loop {
            let Ok((stream, _)) = query_listener.accept().await else {
                break;
            };
            tokio::spawn(serve_dbserver_connection(stream));
        }
    }))
}

async fn serve_dbserver_connection(mut stream: TcpStream) {
    // The bare number field a real player echoes before it will read a
    // message at all — see `Connection::greet`.
    let mut greeting = [0u8; 5];
    if stream.read_exact(&mut greeting).await.is_err() || stream.write_all(&greeting).await.is_err()
    {
        return;
    }

    let mut buffer = Vec::new();

    // SETUP_REQ: any first message acks it, matching `Connection::open`'s
    // `simple_request`, which doesn't check the reply's message type.
    if read_dbserver_message(&mut stream, &mut buffer)
        .await
        .is_none()
    {
        return;
    }
    send_dbserver_message(&mut stream, 0, queries::SETUP_REQ, vec![]).await;

    loop {
        let Some(request) = read_dbserver_message(&mut stream, &mut buffer).await else {
            break;
        };
        match request.message_type {
            queries::METADATA_REQ => {
                // The request type echoed back, then the item count — index 1 is
                // where a real player puts the count.
                send_dbserver_message(
                    &mut stream,
                    1,
                    queries::MENU_AVAILABLE,
                    vec![Field::U32(u32::from(queries::METADATA_REQ)), Field::U32(1)],
                )
                .await;
                let Some(render_req) = read_dbserver_message(&mut stream, &mut buffer).await else {
                    break;
                };
                if render_req.message_type != queries::RENDER_MENU_REQ {
                    break;
                }
                send_dbserver_message(&mut stream, 2, queries::MENU_HEADER, vec![]).await;
                send_dbserver_message(
                    &mut stream,
                    3,
                    queries::MENU_ITEM,
                    vec![
                        Field::U32(1),
                        Field::U32(1),
                        Field::U32(10),
                        Field::Str("Fake Track".to_string()),
                        Field::U32(2),
                        Field::Str(String::new()),
                        Field::U32(0x0004), // TrackTitle
                        Field::U32(0),
                    ],
                )
                .await;
                send_dbserver_message(&mut stream, 4, queries::MENU_FOOTER, vec![]).await;
            }
            queries::BEAT_GRID_REQ => {
                send_dbserver_message(
                    &mut stream,
                    5,
                    queries::BEAT_GRID_RESPONSE,
                    vec![Field::Blob(vec![0u8; 32])],
                )
                .await
            }
            queries::CUE_LIST_EXT_REQ => {
                send_dbserver_message(
                    &mut stream,
                    6,
                    queries::CUE_LIST_EXT_RESPONSE,
                    vec![Field::Blob(vec![0u8; 16])],
                )
                .await
            }
            queries::CUE_LIST_REQ => {
                send_dbserver_message(
                    &mut stream,
                    7,
                    queries::CUE_LIST_RESPONSE,
                    vec![Field::Blob(vec![0u8; 16])],
                )
                .await
            }
            queries::ARTWORK_REQ => {
                send_dbserver_message(
                    &mut stream,
                    8,
                    queries::ARTWORK_RESPONSE,
                    vec![Field::Blob(vec![0u8; 8])],
                )
                .await
            }
            _ => break,
        }
    }
}

async fn read_dbserver_message(stream: &mut TcpStream, buffer: &mut Vec<u8>) -> Option<Message> {
    loop {
        if let Some((message, consumed)) = Message::decode(buffer) {
            buffer.drain(..consumed);
            return Some(message);
        }
        let mut chunk = [0u8; 4096];
        let read = stream.read(&mut chunk).await.ok()?;
        if read == 0 {
            return None;
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
}

async fn send_dbserver_message(
    stream: &mut TcpStream,
    transaction_id: u32,
    message_type: u16,
    args: Vec<Field>,
) {
    let bytes = Message::new(transaction_id, message_type, args).encode();
    let _ = stream.write_all(&bytes).await;
}

#[tokio::test]
async fn phase_d_pulls_metadata_and_blobs_for_a_player_with_a_loaded_track() {
    let _ports = SHARED_NETWORK_PORTS.lock().await;
    let Some(_server) = start_fake_dbserver().await else {
        return;
    };
    let out_dir = tempfile::tempdir().expect("temp out dir");

    let mut capture = spawn_capture(
        out_dir.path(),
        &["--bind", "127.0.0.1", "--duration-sec", "2", "--no-nfs"],
    );
    let injector = tokio::spawn(async {
        for _ in 0..15 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            inject_status_packet().await;
        }
    });

    let status = timeout(Duration::from_secs(30), capture.wait())
        .await
        .expect("prolink-capture should exit on its own")
        .expect("wait");
    injector.abort();
    assert!(status.success(), "prolink-capture exited with {status:?}");

    let session = session_dir(out_dir.path());
    let db_dir = session.join("dbserver").join("127.0.0.1");

    // One report per track: several decks can be playing off one player's
    // media, and they share that player's directory.
    let report_path = db_dir.join(format!("parse-report-{DBSERVER_TRACK_ID}.json"));
    let report: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&report_path)
            .unwrap_or_else(|error| panic!("{}: {error}", report_path.display())),
    )
    .expect("valid json");
    assert_eq!(report["connected"], true, "report: {report}");

    for name in ["beatgrid", "cues-ext", "cues-basic", "artwork"] {
        let path = db_dir.join(format!("{name}-{DBSERVER_TRACK_ID}.bin"));
        let bytes =
            std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        assert!(!bytes.is_empty(), "{} should be non-empty", path.display());
    }
    let metadata_path = db_dir.join(format!("metadata-{DBSERVER_TRACK_ID}.bin"));
    let metadata_bytes = std::fs::read(&metadata_path)
        .unwrap_or_else(|error| panic!("{}: {error}", metadata_path.display()));
    assert!(!metadata_bytes.is_empty());
}

// ---------------------------------------------------------------------
// Deliberately breaking things
// ---------------------------------------------------------------------

/// An unreachable player: nothing is listening on loopback's NFS/dbserver
/// ports in this test, so both connections fail fast and cleanly. A dead
/// player is an expected condition on a live network, not a tool error —
/// the process should still exit 0 with a `parse-report.json` explaining
/// the failure.
#[tokio::test]
async fn an_unreachable_player_produces_a_clean_parse_report_and_a_zero_exit_status() {
    let _ports = SHARED_NETWORK_PORTS.lock().await;
    let out_dir = tempfile::tempdir().expect("temp out dir");
    let mut capture = spawn_capture(
        out_dir.path(),
        &[
            "--bind",
            "127.0.0.1",
            "--player",
            "127.0.0.1",
            "--slot",
            "usb",
            "--duration-sec",
            "1",
        ],
    );
    let status = timeout(Duration::from_secs(60), capture.wait())
        .await
        .expect("prolink-capture should not hang against an unreachable player")
        .expect("wait");
    assert!(
        status.success(),
        "an unreachable player should not fail the process: {status:?}"
    );

    let session = session_dir(out_dir.path());
    let report_path = session
        .join("nfs")
        .join("127.0.0.1-usb")
        .join("parse-report.json");
    let report: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&report_path).expect("parse-report.json should exist"),
    )
    .expect("valid json");
    assert_eq!(report["export_pdb"]["fetched"], false);
    assert!(
        report["export_pdb"]["error"].is_string(),
        "report: {report}"
    );
}

/// `pdb::parse` must never panic on a truncated file — every prefix of a
/// well-formed `export.pdb`, however short, should come back as either a
/// structurally-valid-so-far database or a clean `None`, never a crash. This
/// is the property phase C's `catch_unwind` around `pdb::parse` exists to
/// backstop; asserting it holds directly here is cheaper than only
/// exercising it through the whole binary.
#[test]
fn pdb_parse_never_panics_on_a_truncated_file() {
    let full = support::build_minimal_pdb_bytes();
    for cut in [0, 1, 4, 27, 28, 100, full.len() / 2, full.len() - 1] {
        let _ = pdb::parse(&full[..cut]); // Panicking here fails the test — that is the assertion.
    }
}

/// Killing the tool mid-capture must still leave a readable session
/// directory behind — every JSONL record written so far was already
/// flushed (see `crates/prolink/src/capture.rs`'s module doc), so this
/// asserts that promise holds end to end through a real, forcibly
/// terminated process.
#[tokio::test]
async fn killing_the_tool_mid_capture_leaves_a_readable_session_directory() {
    let _ports = SHARED_NETWORK_PORTS.lock().await;
    let out_dir = tempfile::tempdir().expect("temp out dir");
    let mut capture = spawn_capture(out_dir.path(), &["--no-nfs", "--no-dbserver"]); // No --duration-sec: runs until killed.
    tokio::time::sleep(Duration::from_secs(1)).await;
    capture.kill().await.expect("kill prolink-capture");
    let _ = capture.wait().await;

    let session = session_dir(out_dir.path());
    assert!(session.join("session.json").is_file());
    for log in ["udp.jsonl", "devices.jsonl", "status-decoded.jsonl"] {
        let path = session.join(log);
        // A killed process may have written zero or more complete records;
        // either way this must read back cleanly, not error.
        read_jsonl::<serde_json::Value>(&path).unwrap_or_else(|error| {
            panic!(
                "{} should still be readable after a kill: {error}",
                path.display()
            )
        });
    }
}
