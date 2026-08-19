//! Exercises `prolink::nfs::NfsClient` against a real in-process UDP
//! server — no mocks — same philosophy as
//! `crates/prolink/tests/dbserver_fake_server.rs`. The fake server answers
//! portmapper, MOUNT and NFS calls all on one socket (its GETPORT replies
//! just point back at itself for both programs) — a deliberate testing
//! simplification, not a claim about how real players are wired; it still
//! exercises the client's real call sequence (two portmapper round trips,
//! a mount, a LOOKUP per path segment, a READ loop) end to end.
//!
//! Runs in normal `cargo test`: loopback UDP on an ephemeral port, no
//! privileged port 111 involved (`NfsClient::open_with_portmap_port`
//! exists specifically so tests never need to bind it).

use std::net::Ipv4Addr;
use std::time::Duration;

use prolink::nfs::mount::FileHandle;
use prolink::nfs::procedures::FileAttributes;
use prolink::nfs::rpc::{self, RpcCall};
use prolink::nfs::xdr::XdrDecoder;
use prolink::nfs::{mount, portmap, procedures, NfsClient};
use prolink::status::MediaSlot;
use tokio::net::UdpSocket;
use tokio::time::timeout;

const LOOPBACK: Ipv4Addr = Ipv4Addr::LOCALHOST;
const ROOT_HANDLE: FileHandle = [0xaa; 32];
const PIONEER_HANDLE: FileHandle = [0xbb; 32];
const REKORDBOX_HANDLE: FileHandle = [0xcc; 32];
const PDB_HANDLE: FileHandle = [0xdd; 32];

/// Starts the fake server on an ephemeral port and returns it. `payload` is
/// what `"PIONEER/rekordbox/export.pdb"` resolves to.
async fn start_fake_server(payload: Vec<u8>) -> u16 {
    let socket = UdpSocket::bind((LOOPBACK, 0)).await.expect("bind an ephemeral port");
    let port = socket.local_addr().expect("local addr").port();

    tokio::spawn(async move {
        let mut buf = [0u8; 4096];
        loop {
            let Ok((len, from)) = socket.recv_from(&mut buf).await else { break };
            let Some(call) = rpc::parse_call(&buf[..len]) else { continue };
            if let Some(reply) = handle_call(&call, port, &payload) {
                let _ = socket.send_to(&reply, from).await;
            }
        }
    });

    port
}

fn handle_call(call: &RpcCall<'_>, own_port: u16, payload: &[u8]) -> Option<Vec<u8>> {
    match (call.prog, call.proc) {
        (portmap::PMAP_PROGRAM, _) => Some(portmap::build_getport_reply(call.xid, own_port)),
        (mount::MOUNT_PROGRAM, _) => handle_mount(call),
        (procedures::NFS_PROGRAM, 4) => handle_lookup(call, payload.len() as u32),
        (procedures::NFS_PROGRAM, 6) => handle_read(call, payload),
        _ => None,
    }
}

fn handle_mount(call: &RpcCall<'_>) -> Option<Vec<u8>> {
    let mut decoder = XdrDecoder::new(call.args);
    let path = decoder.read_string_utf16le()?;
    assert_eq!(path, "/C/", "the client should mount a USB slot's own path");
    Some(mount::build_mnt_reply(call.xid, ROOT_HANDLE))
}

fn handle_lookup(call: &RpcCall<'_>, pdb_size: u32) -> Option<Vec<u8>> {
    let mut decoder = XdrDecoder::new(call.args);
    let dir: FileHandle = decoder.read_opaque_fixed(32)?.try_into().ok()?;
    let name = decoder.read_string_utf16le()?;

    let (handle, size) = match (dir, name.as_str()) {
        (h, "PIONEER") if h == ROOT_HANDLE => (PIONEER_HANDLE, 0),
        (h, "rekordbox") if h == PIONEER_HANDLE => (REKORDBOX_HANDLE, 0),
        (h, "export.pdb") if h == REKORDBOX_HANDLE => (PDB_HANDLE, pdb_size),
        _ => return None,
    };
    let attributes = FileAttributes { file_type: procedures::FTYPE_REGULAR_FILE, size };
    Some(procedures::build_lookup_reply(call.xid, handle, attributes))
}

fn handle_read(call: &RpcCall<'_>, payload: &[u8]) -> Option<Vec<u8>> {
    let mut decoder = XdrDecoder::new(call.args);
    let handle: FileHandle = decoder.read_opaque_fixed(32)?.try_into().ok()?;
    if handle != PDB_HANDLE {
        return None;
    }
    let offset = decoder.read_u32()? as usize;
    let count = decoder.read_u32()? as usize;
    let end = (offset + count).min(payload.len());
    let chunk = payload.get(offset..end)?;
    let attributes = FileAttributes { file_type: procedures::FTYPE_REGULAR_FILE, size: payload.len() as u32 };
    Some(procedures::build_read_reply(call.xid, attributes, chunk))
}

#[tokio::test]
async fn fetches_a_multi_chunk_file_via_lookup_chain_and_read_loop() {
    // Larger than one 2048-byte READ chunk, so this also proves the read
    // loop correctly reassembles multiple chunks in order.
    let payload: Vec<u8> = (0..5000u32).map(|i| (i % 256) as u8).collect();
    let port = start_fake_server(payload.clone()).await;

    let mut client = NfsClient::open_with_portmap_port(LOOPBACK, LOOPBACK, MediaSlot::Usb, port)
        .await
        .expect("should open");
    let bytes = client.read_file("PIONEER/rekordbox/export.pdb").await.expect("should fetch");

    assert_eq!(bytes, payload);
}

#[tokio::test]
async fn a_lookup_for_a_path_the_server_does_not_recognize_fails_cleanly() {
    let port = start_fake_server(vec![1, 2, 3]).await;
    let mut client = NfsClient::open_with_portmap_port(LOOPBACK, LOOPBACK, MediaSlot::Usb, port)
        .await
        .expect("should open");

    let result = client.read_file("PIONEER/rekordbox/some-other-file.pdb").await;
    assert!(result.is_err(), "a LOOKUP the server can't resolve should surface as an error, not a hang or a panic");
}

#[tokio::test]
async fn a_server_that_never_answers_is_interrupted_by_a_timeout_rather_than_hanging_forever() {
    let socket = UdpSocket::bind((LOOPBACK, 0)).await.expect("bind an ephemeral port");
    let port = socket.local_addr().expect("local addr").port();
    // Leaked deliberately: keeps the port bound (so the client's sends
    // don't immediately bounce as ICMP port-unreachable, which would give
    // a misleadingly fast failure) with nothing ever reading from it —
    // every request the client makes genuinely goes unanswered.
    std::mem::forget(socket);

    // The client's own retransmit-then-give-up loop (worst case ~3.75s per
    // RPC call, several calls in `open`) should finish well inside this —
    // this outer timeout is a test safety net, not the thing being tested.
    let result = timeout(Duration::from_secs(20), NfsClient::open_with_portmap_port(LOOPBACK, LOOPBACK, MediaSlot::Usb, port)).await;

    match result {
        Ok(Err(_)) => {} // The client's own retry loop gave up — the expected, well-behaved outcome.
        Ok(Ok(_)) => panic!("open() should not succeed against a server that never answers"),
        Err(_) => panic!("open() should give up on its own well within 20s, not need this test's outer timeout"),
    }
}
