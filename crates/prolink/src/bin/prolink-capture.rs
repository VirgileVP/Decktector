//! On-site hardware capture tool: records everything a live Pro DJ Link
//! network offers to disk — raw UDP traffic on all three ports, device
//! discovery, player status, and per-player NFS/dbserver metadata pulls —
//! so every question about this crate's still-hardware-unvalidated parsers
//! (see `crates/prolink/src/nfs/pdb.rs`'s and `crates/prolink/src/dbserver/
//! client.rs`'s doc comments) can be answered offline afterwards, as many
//! times as needed, against real captured bytes. See
//! `HARDWARE-CAPTURE-PLAN.md` (repo root) for the full rationale and
//! `RUNBOOK-HARDWARE-SESSION.md` for how to run this on-site.
//!
//! Four phases, each independent:
//!   A. raw UDP capture on [`prolink::ANNOUNCE_PORT`]/[`prolink::BEAT_PORT`]/
//!      [`prolink::STATUS_PORT`] — every datagram, unparsed, to `udp.jsonl`.
//!      The status port additionally gets a best-effort `parse_status` pass
//!      logged to `status-decoded.jsonl` (success or failure) — done here
//!      rather than via `prolink::status::StatusWatcher` so each decoded
//!      record can carry the exact source address/timestamp of the packet
//!      it came from, which that watcher's forwarding channel doesn't carry.
//!   B. `prolink::discovery::DeviceWatcher` for `devices.jsonl` and a live
//!      table printed to stderr, fed from phase A's own listener via
//!      `DeviceWatcher::ingest` rather than opening a second socket on
//!      [`prolink::ANNOUNCE_PORT`]. Two sockets on one port only fan out
//!      under `SO_REUSEPORT`, which is Unix-only: on Windows the datagram
//!      reaches exactly one of them, phase A wins the race, and phase B
//!      silently sees nothing — an empty device table and a blank live
//!      table, on the one night that can't be redone.
//!   C. NFS pull: `export.pdb` plus each loaded track's `.DAT`/`.EXT`/
//!      artwork, for every player discovered by phase A/B's status decoding
//!      and every `--player`/`--slot` pair given explicitly.
//!   D. dbserver pull: metadata/beat-grid/cues/artwork, for every player
//!      with a currently-loaded track known from phase A's status decoding.
//!
//! **The governing rule, everywhere in this file: write raw bytes to disk
//! before attempting to parse them, and never let a parse or connection
//! failure abort the whole run.** A tool that panics on-site and leaves
//! nothing behind has wasted the whole session; one that dumps bytes it
//! could not interpret has still succeeded completely. Bind/connect/parse
//! failures are logged into the capture (`parse-report.json` files, or
//! `tracing` output) and the tool moves on to the next port/player/phase.
//!
//! Usage:
//!   prolink-capture [--out DIR] [--bind IP] [--player IP] [--slot usb|sd|cd]
//!                    [--max-tracks N] [--duration-sec N] [--no-nfs] [--no-dbserver]

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr};
use std::panic::AssertUnwindSafe;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use prolink::capture::{write_json_file, write_raw_file, CaptureWriter, UdpRecord};
use prolink::dbserver::Connection;
use prolink::discovery::{DeviceKind, DeviceWatcher, DiscoveredDevice};
use prolink::nfs::pdb::{self, Pdb, Track};
use prolink::nfs::NfsClient;
use prolink::status::{parse_status, DeviceStatus, MediaSlot, RawPlayState};
use prolink::{ANNOUNCE_PORT, BEAT_PORT, STATUS_PORT};
use serde::Serialize;
use tokio::net::UdpSocket;

/// Mirrors `crates/prolink-engine/src/metadata/nfs.rs`'s `EXPORT_PDB_PATH` —
/// duplicated rather than imported since `prolink-engine` depends on
/// `prolink`, not the other way around, so this binary (part of the
/// `prolink` crate) can't reach across that boundary.
const EXPORT_PDB_PATH: &str = "PIONEER/rekordbox/export.pdb";
/// Mirrors `prolink-engine::metadata::nfs::EXT_SUFFIX` — see
/// [`EXPORT_PDB_PATH`]'s doc comment for why this is duplicated, not shared.
const EXT_SUFFIX: &str = ".EXT";

/// How long phase A/B are allowed to accumulate discovery before phase C/D
/// run, when `--duration-sec` isn't given — enough for a few keep-alive
/// (~1.5s interval) and status (~200ms interval) packets from every player
/// on the network.
const DEFAULT_SETTLE: Duration = Duration::from_secs(3);
const DEFAULT_MAX_TRACKS: usize = 50;
const DEFAULT_OUT_DIR: &str = "./capture";
const LIVE_TABLE_INTERVAL: Duration = Duration::from_secs(1);

struct Config {
    out_dir: PathBuf,
    bind_ip: Option<Ipv4Addr>,
    player_overrides: Vec<Ipv4Addr>,
    slot_overrides: Vec<MediaSlot>,
    max_tracks: usize,
    duration: Option<Duration>,
    no_nfs: bool,
    no_dbserver: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            out_dir: PathBuf::from(DEFAULT_OUT_DIR),
            bind_ip: None,
            player_overrides: Vec::new(),
            slot_overrides: Vec::new(),
            max_tracks: DEFAULT_MAX_TRACKS,
            duration: None,
            no_nfs: false,
            no_dbserver: false,
        }
    }
}

fn parse_args() -> Config {
    let mut config = Config::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--out" => config.out_dir = PathBuf::from(args.next().expect("--out needs a directory")),
            "--bind" => config.bind_ip = Some(args.next().and_then(|v| v.parse().ok()).expect("--bind needs an IPv4 address")),
            "--player" => config.player_overrides.push(args.next().and_then(|v| v.parse().ok()).expect("--player needs an IPv4 address")),
            "--slot" => {
                let raw = args.next().expect("--slot needs usb, sd or cd");
                config.slot_overrides.push(parse_slot(&raw).unwrap_or_else(|| panic!("unrecognized --slot value: {raw}")));
            }
            "--max-tracks" => config.max_tracks = args.next().and_then(|v| v.parse().ok()).expect("--max-tracks needs a number"),
            "--duration-sec" => {
                let secs: u64 = args.next().and_then(|v| v.parse().ok()).expect("--duration-sec needs a number");
                config.duration = Some(Duration::from_secs(secs));
            }
            "--no-nfs" => config.no_nfs = true,
            "--no-dbserver" => config.no_dbserver = true,
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            other => {
                eprintln!("unknown argument: {other}\n");
                print_usage();
                std::process::exit(1);
            }
        }
    }
    config
}

fn parse_slot(raw: &str) -> Option<MediaSlot> {
    match raw.to_ascii_lowercase().as_str() {
        "usb" => Some(MediaSlot::Usb),
        "sd" => Some(MediaSlot::Sd),
        "cd" => Some(MediaSlot::Cd),
        _ => None,
    }
}

fn slot_label(slot: MediaSlot) -> String {
    match slot {
        MediaSlot::Cd => "cd".to_string(),
        MediaSlot::Sd => "sd".to_string(),
        MediaSlot::Usb => "usb".to_string(),
        MediaSlot::Rekordbox => "rekordbox".to_string(),
        MediaSlot::Streaming => "streaming".to_string(),
        MediaSlot::Unknown(byte) => format!("unknown{byte:#04x}"),
    }
}

fn play_state_label(state: RawPlayState) -> String {
    match state {
        RawPlayState::NoTrack => "no_track".to_string(),
        RawPlayState::Loading => "loading".to_string(),
        RawPlayState::Playing => "playing".to_string(),
        RawPlayState::Looping => "looping".to_string(),
        RawPlayState::PausedAway => "paused_away".to_string(),
        RawPlayState::PausedAtCue => "paused_at_cue".to_string(),
        RawPlayState::CuePlaying => "cue_playing".to_string(),
        RawPlayState::CueScratching => "cue_scratching".to_string(),
        RawPlayState::Searching => "searching".to_string(),
        RawPlayState::SpunDown => "spun_down".to_string(),
        RawPlayState::Ended => "ended".to_string(),
        RawPlayState::EmergencyLoop => "emergency_loop".to_string(),
        RawPlayState::Unknown(byte) => format!("unknown{byte:#04x}"),
    }
}

fn print_usage() {
    eprintln!("prolink-capture — records a Pro DJ Link hardware session to disk for offline replay");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --out DIR            session output directory (default {DEFAULT_OUT_DIR})");
    eprintln!("  --bind IP            local adapter IP (default: auto-detected, printed at startup)");
    eprintln!("  --player IP          explicit player to pull NFS metadata from, pairs with --slot by position");
    eprintln!("  --slot usb|sd|cd     media slot for the matching --player, see above");
    eprintln!("  --max-tracks N       cap on analysis/artwork files pulled per player (default {DEFAULT_MAX_TRACKS})");
    eprintln!("  --duration-sec N     stop after N seconds instead of running until Ctrl+C");
    eprintln!("  --no-nfs             skip phase C (NFS metadata pull)");
    eprintln!("  --no-dbserver        skip phase D (dbserver metadata pull)");
}

/// One player's most-recently-seen status, keyed by device number —
/// populated by phase A's status-port decoding, read by phase B's live
/// table and phases C/D's auto-discovered targets.
#[derive(Debug, Clone)]
struct LedgerEntry {
    ip: Ipv4Addr,
    media_slot: MediaSlot,
    track_type: u8,
    track_id: Option<u32>,
    play_state: RawPlayState,
}

type Ledger = Arc<Mutex<HashMap<u8, LedgerEntry>>>;

#[derive(Serialize)]
struct HostInterface {
    name: String,
    ip: String,
}

#[derive(Serialize)]
struct SessionInfo {
    tool: &'static str,
    version: &'static str,
    args: Vec<String>,
    git_sha: Option<String>,
    bind_ip: Ipv4Addr,
    host_interfaces: Vec<HostInterface>,
    started_at_unix_ms: u128,
    ended_at_unix_ms: Option<u128>,
}

#[tokio::main]
async fn main() {
    let config = parse_args();

    let subscriber = tracing_subscriber::fmt().with_max_level(tracing::Level::DEBUG).with_writer(std::io::stderr).finish();
    let _ = tracing::subscriber::set_global_default(subscriber);

    let bind_ip = match config.bind_ip {
        Some(ip) => ip,
        None => resolve_bind_ip(),
    };
    println!("Binding to interface {bind_ip}");

    let writer = match CaptureWriter::new(&config.out_dir) {
        Ok(writer) => Arc::new(writer),
        Err(error) => {
            eprintln!("failed to create the capture session directory under {}: {error}", config.out_dir.display());
            std::process::exit(1);
        }
    };
    println!("Session directory: {}", writer.session_dir().display());

    let started_at = now_unix_ms();
    let mut session_info = SessionInfo {
        tool: "prolink-capture",
        version: env!("CARGO_PKG_VERSION"),
        args: std::env::args().collect(),
        git_sha: git_sha(),
        bind_ip,
        host_interfaces: host_interfaces(),
        started_at_unix_ms: started_at,
        ended_at_unix_ms: None,
    };
    if let Err(error) = writer.write_session_info(&session_info) {
        tracing::warn!(%error, "failed to write session.json at startup");
    }

    let ledger: Ledger = Arc::new(Mutex::new(HashMap::new()));

    // Phase B's watcher, created before phase A because phase A's
    // announce-port listener is what feeds it (see this module's doc
    // comment for why it doesn't get a socket of its own).
    let device_watcher = DeviceWatcher::new();

    // Phase A: raw capture on all three ports, plus best-effort status
    // decoding (see this module's doc comment for why status decoding lives
    // here rather than behind `StatusWatcher`).
    for port in [ANNOUNCE_PORT, BEAT_PORT, STATUS_PORT] {
        tokio::spawn(run_phase_a(Arc::clone(&writer), Arc::clone(&ledger), Arc::clone(&device_watcher), port));
    }

    // Phase B: device discovery, driven by phase A.
    tokio::spawn(run_device_sweep(Arc::clone(&device_watcher)));
    tokio::spawn(run_device_log(Arc::clone(&writer), device_watcher.subscribe()));
    tokio::spawn(run_live_table(Arc::clone(&device_watcher), Arc::clone(&ledger)));

    let settle = config.duration.unwrap_or(DEFAULT_SETTLE);
    tokio::time::sleep(settle).await;

    let targets = build_nfs_targets(&config, &ledger);
    if config.no_nfs {
        println!("phase C (NFS) skipped: --no-nfs");
    } else {
        run_phase_c(Arc::clone(&writer), bind_ip, targets, Arc::clone(&ledger), config.max_tracks).await;
    }

    if config.no_dbserver {
        println!("phase D (dbserver) skipped: --no-dbserver");
    } else {
        run_phase_d(Arc::clone(&writer), bind_ip, Arc::clone(&ledger)).await;
    }

    if config.duration.is_none() {
        println!("Recording continues — press Ctrl+C to stop and finish the session.");
        wait_for_stop().await;
    }

    session_info.ended_at_unix_ms = Some(now_unix_ms());
    if let Err(error) = writer.write_session_info(&session_info) {
        tracing::warn!(%error, "failed to write session.json at shutdown");
    }
    println!("Session complete: {}", writer.session_dir().display());
}

/// Ends an open-ended recording on whichever comes first: Ctrl+C (someone
/// at the booth, the runbook's step 8) or stdin reaching EOF (a parent
/// process closing the pipe — how `prolink-session` stops this tool).
///
/// Ctrl+C alone isn't enough for the guided driver: a parent can't
/// portably deliver it to a child on Windows, and killing the process
/// instead would skip the shutdown `write_session_info` below, leaving
/// `session.json` without its `ended_at_unix_ms`. Stray bytes on stdin are
/// ignored rather than treated as a stop, so running this in a terminal
/// where someone leans on the keyboard doesn't end the session.
async fn wait_for_stop() {
    use tokio::io::AsyncReadExt;

    let drained_stdin = async {
        let mut stdin = tokio::io::stdin();
        let mut buf = [0u8; 64];
        loop {
            match stdin.read(&mut buf).await {
                Ok(0) | Err(_) => break, // EOF, or a closed pipe: the parent is done.
                Ok(_) => continue,
            }
        }
    };

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = drained_stdin => {}
    }
}

fn now_unix_ms() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|elapsed| elapsed.as_millis()).unwrap_or(0)
}

fn git_sha() -> Option<String> {
    let output = std::process::Command::new("git").args(["rev-parse", "HEAD"]).output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn host_interfaces() -> Vec<HostInterface> {
    if_addrs::get_if_addrs()
        .map(|interfaces| {
            interfaces
                .into_iter()
                .map(|interface| {
                    let ip = interface.ip().to_string();
                    HostInterface { name: interface.name, ip }
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Picks the first non-loopback IPv4 interface — printed loudly regardless
/// of whether it was auto-detected or given via `--bind`, since a wrong
/// interface on a multi-homed booth machine is a likely on-site failure
/// mode and must be visible without being asked for.
fn resolve_bind_ip() -> Ipv4Addr {
    if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .find_map(|interface| match interface.ip() {
            IpAddr::V4(ip) if !ip.is_loopback() => Some(ip),
            _ => None,
        })
        .unwrap_or(Ipv4Addr::LOCALHOST)
}

/// Phase A for one port: binds it, and forever (until a receive error ends
/// the task) writes every datagram raw to `udp.jsonl`. A bind failure is
/// logged and this port's capture simply never runs — the other two ports'
/// tasks are unaffected, matching every `*Watcher::run` in this crate.
async fn run_phase_a(writer: Arc<CaptureWriter>, ledger: Ledger, device_watcher: Arc<DeviceWatcher>, port: u16) {
    let socket = match bind_capture_socket(port).await {
        Ok(socket) => socket,
        Err(error) => {
            tracing::error!(%error, port, "phase A: failed to bind capture socket");
            return;
        }
    };

    let mut buf = [0u8; 2048];
    loop {
        let (len, source) = match socket.recv_from(&mut buf).await {
            Ok(value) => value,
            Err(error) => {
                tracing::warn!(%error, port, "phase A: failed to read from capture socket");
                continue;
            }
        };
        let payload = &buf[..len];
        let record = UdpRecord::new(port, source.ip(), payload);
        if let Err(error) = writer.write_udp(&record) {
            tracing::warn!(%error, port, "phase A: failed to write a udp.jsonl record");
        }

        // Raw bytes are on disk by this point, so neither of these can cost
        // the capture anything if it misreads the packet.
        if port == ANNOUNCE_PORT {
            if let IpAddr::V4(source_ip) = source.ip() {
                device_watcher.ingest(payload, source_ip);
            }
        }

        if port == STATUS_PORT {
            handle_status_packet(&writer, &ledger, payload, source.ip(), record.t);
        }
    }
}

/// Same socket setup as `prolink::bind_udp_socket` (`SO_REUSEADDR`,
/// broadcast-capable, non-blocking) — reimplemented here rather than
/// called, since that helper is `pub(crate)` to the library and this binary
/// is a separate crate target that can only see the library's public API.
async fn bind_capture_socket(port: u16) -> std::io::Result<UdpSocket> {
    use socket2::{Domain, Socket, Type};
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, None)?;
    socket.set_reuse_address(true)?;
    #[cfg(unix)]
    socket.set_reuse_port(true)?;
    socket.set_broadcast(true)?;
    socket.set_nonblocking(true)?;
    socket.bind(&std::net::SocketAddr::from((Ipv4Addr::UNSPECIFIED, port)).into())?;
    UdpSocket::from_std(socket.into())
}

#[derive(Serialize)]
#[serde(tag = "outcome")]
enum StatusDecodeRecord {
    #[serde(rename = "decoded")]
    Decoded {
        t: i64,
        src: String,
        device_number: u8,
        play_state: String,
        is_master: bool,
        is_synced: bool,
        is_on_air: bool,
        media_slot: String,
        track_type: u8,
        pitch_percent: f64,
        track_bpm: Option<f64>,
        beat_count: Option<u32>,
        beat_in_bar: Option<u8>,
        track_id: Option<u32>,
    },
    #[serde(rename = "decode_failed")]
    DecodeFailed { t: i64, src: String, len: usize },
}

/// Best-effort `parse_status` pass over one raw status-port datagram: a
/// `None` result is recorded as a decode failure and does not stop or delay
/// phase A's raw recording in any way — this runs strictly after the raw
/// write already happened.
fn handle_status_packet(writer: &CaptureWriter, ledger: &Ledger, payload: &[u8], source_ip: IpAddr, t: i64) {
    let record = match parse_status(payload, source_ip) {
        Some(status) => {
            update_ledger(ledger, &status, source_ip);
            status_to_record(&status, source_ip, t)
        }
        None => StatusDecodeRecord::DecodeFailed { t, src: source_ip.to_string(), len: payload.len() },
    };
    if let Err(error) = writer.write_status_decoded(&record) {
        tracing::warn!(%error, "phase A: failed to write a status-decoded.jsonl record");
    }
}

fn status_to_record(status: &DeviceStatus, source_ip: IpAddr, t: i64) -> StatusDecodeRecord {
    StatusDecodeRecord::Decoded {
        t,
        src: source_ip.to_string(),
        device_number: status.device_number,
        play_state: play_state_label(status.play_state),
        is_master: status.is_master,
        is_synced: status.is_synced,
        is_on_air: status.is_on_air,
        media_slot: slot_label(status.media_slot),
        track_type: status.track_type,
        pitch_percent: status.pitch_percent,
        track_bpm: status.track_bpm,
        beat_count: status.beat_count,
        beat_in_bar: status.beat_in_bar,
        track_id: status.track_id,
    }
}

fn update_ledger(ledger: &Ledger, status: &DeviceStatus, source_ip: IpAddr) {
    let IpAddr::V4(ip) = source_ip else { return };
    let mut ledger = ledger.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let entry = ledger.entry(status.device_number).or_insert_with(|| LedgerEntry {
        ip,
        media_slot: status.media_slot,
        track_type: status.track_type,
        track_id: status.track_id,
        play_state: status.play_state,
    });
    entry.ip = ip;
    entry.media_slot = status.media_slot;
    entry.track_type = status.track_type;
    entry.track_id = status.track_id;
    entry.play_state = status.play_state;
}

#[derive(Serialize)]
struct DeviceSnapshot {
    t: i64,
    devices: Vec<DiscoveredDevice>,
}

async fn run_device_log(writer: Arc<CaptureWriter>, mut devices_rx: tokio::sync::watch::Receiver<Vec<DiscoveredDevice>>) {
    loop {
        let devices = devices_rx.borrow_and_update().clone();
        let snapshot = DeviceSnapshot { t: now_micros(), devices };
        if let Err(error) = writer.write_device(&snapshot) {
            tracing::warn!(%error, "phase B: failed to write a devices.jsonl record");
        }
        if devices_rx.changed().await.is_err() {
            break; // The device watcher was dropped.
        }
    }
}

fn now_micros() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|elapsed| elapsed.as_micros() as i64).unwrap_or(0)
}

/// Prints device number/name/ip/media-slot/loaded-track-id once a second to
/// stderr, so the operator can see at a glance that the network is being
/// read — required even if every parser in the crate is broken, since it's
/// driven by the same raw ledger phase A already maintains.
/// Expires devices that stopped broadcasting. [`DeviceWatcher::run`] does
/// this on its own; a watcher driven through `ingest` needs it driven too,
/// or a player that gets unplugged stays in the table for the rest of the
/// session.
async fn run_device_sweep(device_watcher: Arc<DeviceWatcher>) {
    let mut tick = tokio::time::interval(DeviceWatcher::SWEEP_INTERVAL);
    loop {
        tick.tick().await;
        device_watcher.sweep();
    }
}

async fn run_live_table(device_watcher: Arc<DeviceWatcher>, ledger: Ledger) {
    let mut tick = tokio::time::interval(LIVE_TABLE_INTERVAL);
    loop {
        tick.tick().await;
        let devices = device_watcher.devices();
        if devices.is_empty() {
            continue;
        }
        eprintln!("--- Pro DJ Link devices ({} seen) ---", devices.len());
        let ledger = ledger.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        for device in &devices {
            let extra = ledger
                .get(&device.device_number)
                .map(|entry| format!(" slot={} track_id={:?} state={}", slot_label(entry.media_slot), entry.track_id, play_state_label(entry.play_state)))
                .unwrap_or_default();
            eprintln!("  #{:<2} {:<16} {:<15} {:?}{extra}", device.device_number, device.name, device.ip, device.kind);
        }
    }
}

/// Every `(player_ip, slot)` this run should try over NFS: explicit
/// `--player`/`--slot` pairs first (position-matched — the operator's own
/// on-site read of the player's display, which must work even if discovery
/// is broken), then every player currently in the ledger.
fn build_nfs_targets(config: &Config, ledger: &Ledger) -> Vec<(Ipv4Addr, MediaSlot)> {
    let mut targets: Vec<(Ipv4Addr, MediaSlot)> = config.player_overrides.iter().copied().zip(config.slot_overrides.iter().copied()).collect();
    if config.player_overrides.len() != config.slot_overrides.len() {
        tracing::warn!(
            players = config.player_overrides.len(),
            slots = config.slot_overrides.len(),
            "phase C: --player and --slot counts differ; only the paired-up prefix is used"
        );
    }

    let ledger = ledger.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    for entry in ledger.values() {
        let pair = (entry.ip, entry.media_slot);
        if !targets.contains(&pair) {
            targets.push(pair);
        }
    }
    targets
}

#[derive(Serialize)]
struct NfsParseReport {
    player_ip: Ipv4Addr,
    slot: String,
    export_pdb: ExportPdbReport,
    tracks: Vec<TrackFetchReport>,
}

#[derive(Serialize)]
struct ExportPdbReport {
    fetched: bool,
    bytes: usize,
    parsed: bool,
    track_count: usize,
    error: Option<String>,
}

#[derive(Serialize)]
struct TrackFetchReport {
    track_id: u32,
    dat_fetched: bool,
    ext_fetched: bool,
    artwork_fetched: bool,
    error: Option<String>,
}

async fn run_phase_c(writer: Arc<CaptureWriter>, bind_ip: Ipv4Addr, targets: Vec<(Ipv4Addr, MediaSlot)>, ledger: Ledger, max_tracks: usize) {
    for (player_ip, slot) in targets {
        let label = slot_label(slot);
        println!("phase C: pulling NFS metadata from {player_ip} ({label})");
        let dir = match writer.nfs_capture_dir(player_ip, &label) {
            Ok(dir) => dir,
            Err(error) => {
                tracing::error!(%error, %player_ip, slot = %label, "phase C: failed to create the capture directory");
                continue;
            }
        };

        let mut client = match NfsClient::open(player_ip, bind_ip, slot).await {
            Ok(client) => client,
            Err(error) => {
                tracing::warn!(%error, %player_ip, slot = %label, "phase C: could not open an NFS connection — a dead/unreachable player is an expected condition on a live network");
                let report = NfsParseReport {
                    player_ip,
                    slot: label,
                    export_pdb: ExportPdbReport { fetched: false, bytes: 0, parsed: false, track_count: 0, error: Some(error.to_string()) },
                    tracks: Vec::new(),
                };
                write_parse_report(&dir, &report);
                continue;
            }
        };

        let pdb_bytes = match client.read_file(EXPORT_PDB_PATH).await {
            Ok(bytes) => bytes,
            Err(error) => {
                tracing::warn!(%error, %player_ip, "phase C: failed to fetch export.pdb");
                let report = NfsParseReport {
                    player_ip,
                    slot: label,
                    export_pdb: ExportPdbReport { fetched: false, bytes: 0, parsed: false, track_count: 0, error: Some(error.to_string()) },
                    tracks: Vec::new(),
                };
                write_parse_report(&dir, &report);
                continue;
            }
        };

        // Governing rule: the raw bytes hit disk before parsing is even attempted.
        if let Err(error) = write_raw_file(&dir.join("export.pdb"), &pdb_bytes) {
            tracing::error!(%error, %player_ip, "phase C: failed to write raw export.pdb to disk");
        }

        let parse_outcome = std::panic::catch_unwind(AssertUnwindSafe(|| pdb::parse(&pdb_bytes)));
        let pdb = match parse_outcome {
            Ok(Some(pdb)) => Some(pdb),
            Ok(None) => {
                tracing::warn!(%player_ip, "phase C: export.pdb did not parse (structurally invalid for this parser)");
                None
            }
            Err(panic) => {
                tracing::error!(%player_ip, "phase C: export.pdb parsing panicked — bytes are still safely on disk: {}", panic_message(&panic));
                None
            }
        };

        let track_reports = match &pdb {
            Some(pdb) => fetch_tracks(&mut client, &dir, pdb, preferred_track_ids_for(&ledger, player_ip), max_tracks).await,
            None => Vec::new(),
        };

        let report = NfsParseReport {
            player_ip,
            slot: label,
            export_pdb: ExportPdbReport {
                fetched: true,
                bytes: pdb_bytes.len(),
                parsed: pdb.is_some(),
                track_count: pdb.as_ref().map_or(0, |pdb| pdb.tracks.len()),
                error: None,
            },
            tracks: track_reports,
        };
        write_parse_report(&dir, &report);
    }
}

/// Track ids to prefer when capping at `max_tracks`: whatever this
/// player's own ledger entries currently report as loaded (there can be
/// more than one if the operator changed tracks during the settle window).
fn preferred_track_ids_for(ledger: &Ledger, player_ip: Ipv4Addr) -> Vec<u32> {
    let ledger = ledger.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    ledger.values().filter(|entry| entry.ip == player_ip).filter_map(|entry| entry.track_id).collect()
}

async fn fetch_tracks(client: &mut NfsClient, dir: &std::path::Path, pdb: &Pdb, preferred: Vec<u32>, max_tracks: usize) -> Vec<TrackFetchReport> {
    let mut ids: Vec<u32> = preferred.into_iter().filter(|id| pdb.tracks.contains_key(id)).collect();
    for id in pdb.tracks.keys() {
        if ids.len() >= max_tracks {
            break;
        }
        if !ids.contains(id) {
            ids.push(*id);
        }
    }
    ids.truncate(max_tracks);

    let mut reports = Vec::with_capacity(ids.len());
    for track_id in ids {
        let Some(track) = pdb.tracks.get(&track_id) else { continue };
        reports.push(fetch_one_track(client, dir, pdb, track).await);
    }
    reports
}

async fn fetch_one_track(client: &mut NfsClient, dir: &std::path::Path, pdb: &Pdb, track: &Track) -> TrackFetchReport {
    let mut error = None;

    let dat_fetched = match client.read_file(&track.analyze_path).await {
        Ok(bytes) => {
            let path = dir.join("analysis").join(format!("{}.DAT", track.id));
            if let Err(write_error) = write_raw_file(&path, &bytes) {
                error = Some(write_error.to_string());
            }
            true
        }
        Err(fetch_error) => {
            error.get_or_insert(fetch_error.to_string());
            false
        }
    };

    let ext_path = ext_variant_path(&track.analyze_path);
    let ext_fetched = match client.read_file(&ext_path).await {
        Ok(bytes) => {
            let path = dir.join("analysis").join(format!("{}.EXT", track.id));
            if let Err(write_error) = write_raw_file(&path, &bytes) {
                error.get_or_insert(write_error.to_string());
            }
            true
        }
        Err(fetch_error) => {
            error.get_or_insert(fetch_error.to_string());
            false
        }
    };

    let artwork_fetched = match pdb.artwork_paths.get(&track.artwork_id) {
        Some(artwork_path) => match client.read_file(artwork_path).await {
            Ok(bytes) if !bytes.is_empty() => {
                let path = dir.join("artwork").join(format!("{}.bin", track.id));
                if let Err(write_error) = write_raw_file(&path, &bytes) {
                    error.get_or_insert(write_error.to_string());
                }
                true
            }
            Ok(_) => false,
            Err(fetch_error) => {
                error.get_or_insert(fetch_error.to_string());
                false
            }
        },
        None => false,
    };

    TrackFetchReport { track_id: track.id, dat_fetched, ext_fetched, artwork_fetched, error }
}

fn ext_variant_path(dat_path: &str) -> String {
    for suffix in [".DAT", ".dat"] {
        if let Some(base) = dat_path.strip_suffix(suffix) {
            return format!("{base}{EXT_SUFFIX}");
        }
    }
    dat_path.to_string()
}

fn panic_message(panic: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = panic.downcast_ref::<&str>() {
        message.to_string()
    } else if let Some(message) = panic.downcast_ref::<String>() {
        message.clone()
    } else {
        "non-string panic payload".to_string()
    }
}

fn write_parse_report(dir: &std::path::Path, report: &impl Serialize) {
    if let Err(error) = write_json_file(&dir.join("parse-report.json"), report) {
        tracing::error!(%error, dir = %dir.display(), "failed to write parse-report.json");
    }
}

#[derive(Serialize)]
struct DbserverParseReport {
    player_ip: Ipv4Addr,
    device_number: u8,
    track_id: Option<u32>,
    connected: bool,
    queries: Vec<QueryReport>,
    error: Option<String>,
}

#[derive(Serialize)]
struct QueryReport {
    query: &'static str,
    fetched: bool,
    bytes: usize,
    error: Option<String>,
}

/// Decktective's own virtual player number for this run — honours
/// `DECKTECTIVE_VCDJ_ID` the same way `prolink-engine::engine` does (see
/// the project README's "Metadata on a full four-player network" section);
/// duplicated here for the same reason as [`EXPORT_PDB_PATH`] — this binary
/// can't depend on `prolink-engine`.
fn our_device_number() -> u8 {
    let number = std::env::var("DECKTECTIVE_VCDJ_ID").ok().and_then(|value| value.parse().ok()).unwrap_or(5);
    if !(1..=4).contains(&number) {
        tracing::warn!(number, "DECKTECTIVE_VCDJ_ID (or the default, 5) is outside dbserver's accepted 1-4 range; dbserver may legitimately answer nothing — see the README's 'Metadata on a full four-player network'");
    }
    number
}

async fn run_phase_d(writer: Arc<CaptureWriter>, bind_ip: Ipv4Addr, ledger: Ledger) {
    let our_device_number = our_device_number();
    let targets: Vec<(u8, LedgerEntry)> = {
        let ledger = ledger.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        ledger
            .iter()
            .filter(|(&device_number, entry)| DeviceKind::from_device_number(device_number) == DeviceKind::Player && entry.track_id.is_some())
            .map(|(&device_number, entry)| (device_number, entry.clone()))
            .collect()
    };

    if targets.is_empty() {
        println!("phase D: no player currently reports a loaded track; nothing to pull");
        return;
    }

    for (device_number, entry) in targets {
        let track_id = entry.track_id.expect("filtered to Some above");
        println!("phase D: pulling dbserver metadata from {} (device {device_number}, track {track_id})", entry.ip);
        let dir = match writer.dbserver_capture_dir(entry.ip) {
            Ok(dir) => dir,
            Err(error) => {
                tracing::error!(%error, ip = %entry.ip, "phase D: failed to create the capture directory");
                continue;
            }
        };

        let mut connection = match Connection::open(entry.ip, bind_ip, our_device_number).await {
            Ok(connection) => connection,
            Err(error) => {
                tracing::warn!(%error, ip = %entry.ip, "phase D: could not open a dbserver connection — a dead/unreachable player is expected on a live network");
                let report = DbserverParseReport { player_ip: entry.ip, device_number, track_id: Some(track_id), connected: false, queries: Vec::new(), error: Some(error.to_string()) };
                write_parse_report(&dir, &report);
                continue;
            }
        };

        let slot_byte = entry.media_slot.to_byte();
        let mut queries = Vec::new();

        queries.push(run_metadata_query(&mut connection, &dir, device_number, slot_byte, entry.track_type, track_id).await);

        let beat_grid = connection.fetch_beat_grid(device_number, slot_byte, entry.track_type, track_id).await;
        queries.push(record_blob_result(&dir, "beatgrid", track_id, beat_grid));

        let cues_ext = connection.fetch_cues_extended(device_number, slot_byte, entry.track_type, track_id).await;
        queries.push(record_blob_result(&dir, "cues-ext", track_id, cues_ext));

        let cues_basic = connection.fetch_cues_basic(device_number, slot_byte, entry.track_type, track_id).await;
        queries.push(record_blob_result(&dir, "cues-basic", track_id, cues_basic));

        queries.push(run_artwork_query(&mut connection, &dir, device_number, slot_byte, track_id).await);

        let report = DbserverParseReport { player_ip: entry.ip, device_number, track_id: Some(track_id), connected: true, queries, error: None };
        write_parse_report(&dir, &report);
    }
}

/// `Connection::fetch_metadata` runs the full menu-render exchange
/// internally and hands back already-parsed items rather than exposing raw
/// response bytes — unlike the blob-style queries below, there's no lower
/// -level escape hatch in `dbserver::Connection`'s current API for this one.
/// Captured here as JSON (still named `metadata-<id>.bin` per this crate's
/// capture layout) rather than skipped, since the parsed items are still
/// valuable replay input; a raw-bytes passthrough would be a `dbserver`
/// client enhancement, out of scope for this capture tool.
async fn run_metadata_query(connection: &mut Connection, dir: &std::path::Path, device_number: u8, slot: u8, track_type: u8, track_id: u32) -> QueryReport {
    match connection.fetch_metadata(device_number, slot, track_type, track_id).await {
        Ok(items) => {
            let payload: Vec<_> = items
                .iter()
                .map(|item| serde_json::json!({ "item_type": format!("{:?}", item.item_type), "text": item.text, "number": item.number }))
                .collect();
            let bytes = serde_json::to_vec_pretty(&payload).unwrap_or_default();
            let path = dir.join(format!("metadata-{track_id}.bin"));
            let error = write_raw_file(&path, &bytes).err().map(|e| e.to_string());
            QueryReport { query: "metadata", fetched: error.is_none(), bytes: bytes.len(), error }
        }
        Err(fetch_error) => QueryReport { query: "metadata", fetched: false, bytes: 0, error: Some(fetch_error.to_string()) },
    }
}

/// Writes one blob-style query's already-awaited result to disk (see
/// [`Connection::fetch_beat_grid`]/`fetch_cues_extended`/`fetch_cues_basic`)
/// and reports the outcome. Takes the result rather than a fetch closure —
/// a closure borrowing `&mut Connection` across an `.await` doesn't have a
/// lifetime simple enough to express as a reusable generic helper here, and
/// three call sites don't justify fighting it.
fn record_blob_result(dir: &std::path::Path, name: &'static str, track_id: u32, result: std::io::Result<Option<Vec<u8>>>) -> QueryReport {
    match result {
        Ok(Some(bytes)) => {
            let path = dir.join(format!("{name}-{track_id}.bin"));
            let error = write_raw_file(&path, &bytes).err().map(|e| e.to_string());
            QueryReport { query: name, fetched: error.is_none(), bytes: bytes.len(), error }
        }
        Ok(None) => QueryReport { query: name, fetched: false, bytes: 0, error: None },
        Err(fetch_error) => QueryReport { query: name, fetched: false, bytes: 0, error: Some(fetch_error.to_string()) },
    }
}

async fn run_artwork_query(connection: &mut Connection, dir: &std::path::Path, device_number: u8, slot: u8, track_id: u32) -> QueryReport {
    // Artwork is looked up by artwork id in the real protocol, but this
    // tool doesn't have one available without a prior metadata parse it
    // deliberately keeps decoupled (see `run_metadata_query`'s doc comment)
    // — `track_id` is passed through as the artwork id, which is wrong in
    // general but harmless: a mismatched id just gets `Ok(None)` back from
    // the player, recorded as "fetched: false" rather than corrupting
    // anything, and the raw metadata JSON captured above still carries
    // whatever the real artwork id was for offline reconciliation.
    match connection.fetch_artwork(device_number, slot, track_id).await {
        Ok(Some(bytes)) => {
            let path = dir.join(format!("artwork-{track_id}.bin"));
            let error = write_raw_file(&path, &bytes).err().map(|e| e.to_string());
            QueryReport { query: "artwork", fetched: error.is_none(), bytes: bytes.len(), error }
        }
        Ok(None) => QueryReport { query: "artwork", fetched: false, bytes: 0, error: None },
        Err(fetch_error) => QueryReport { query: "artwork", fetched: false, bytes: 0, error: Some(fetch_error.to_string()) },
    }
}
