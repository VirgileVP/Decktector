//! Standalone dev tool that broadcasts fake Pro DJ Link traffic — keep
//! -alives, status packets and beat packets — so decktector's real engine
//! (`prolink-engine::ProlinkEngine`) can be exercised against something that
//! actually speaks the wire protocol, without real CDJs on the network.
//! Unlike `decktector_core::sim::SimEngine`, which bypasses the protocol
//! entirely, this makes `cargo run -p decktector` (with `DECKTECTOR_SIM`
//! unset) show live decks end to end through the real parsing path. Run it
//! on the same machine as decktector — no virtual-CDJ startup dance, just
//! the same broadcasts a real player settles into once it's up.
//!
//! It can also stop inventing traffic altogether and **replay a recorded
//! one** (`--replay DIR`, a session directory from `prolink-capture` or
//! `prolink-pcap`), sending every datagram back out with the timing it was
//! recorded with. Generated traffic is regular in a way a real booth never
//! is — a fixed tempo, an unwavering status rate, no pitch bends, no
//! loading, no gaps — so replay is what exercises the parsing path against
//! what actually happens on a stage.
//!
//! One difference is unavoidable and worth knowing: the replayed packets
//! come from this machine, so the source address a receiver sees is the
//! replay target rather than the player's own. Device identity is not
//! affected — status packets carry the device number, and the app keys off
//! that — but a discovered device's *address* is the address it is replayed
//! from.
//!
//! Usage:
//!   cdj-sim [--count N] [--names CDJ-2000,CDJ-3000] [--mixer]
//!           [--interval-ms 1500] [--broadcast 255.255.255.255]
//!           [--bpm 128] [--precise-position]
//!   cdj-sim --replay DIR [--replay-target 127.0.0.1] [--speed 1.0] [--loop]

use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use prolink::beat::{build_beat_packet, build_precise_position};
use prolink::capture::{stream_jsonl, UdpRecord};
use prolink::discovery::{build_keepalive, DeviceKind};
use prolink::status::{build_status, RawPlayState};
use prolink::{ANNOUNCE_PORT, BEAT_PORT, STATUS_PORT};
use tokio::net::UdpSocket;
use tokio::time;

/// How often a status packet goes out per simulated deck — matches a real
/// player's own rate (see `prolink::status`'s module doc).
const STATUS_INTERVAL: Duration = Duration::from_millis(200);
/// How long a freshly "loaded" deck sits cued before this tool starts it
/// playing — long enough to see the cued state in the UI before it moves.
const CUE_DURATION: Duration = Duration::from_secs(2);

/// `udp.jsonl` within a session directory: the file replay reads.
const UDP_LOG: &str = "udp.jsonl";
/// How often replay reports progress. A recorded evening runs to hundreds
/// of thousands of datagrams, and silence for minutes looks like a hang.
const REPLAY_PROGRESS_EVERY: u64 = 20_000;

struct SimulatedDevice {
    device_number: u8,
    name: String,
}

struct Config {
    devices: Vec<SimulatedDevice>,
    interval: Duration,
    broadcast_addr: Ipv4Addr,
    bpm: f64,
    precise_position: bool,
    replay: Option<ReplayConfig>,
}

/// Set only by `--replay`, and mutually exclusive with everything the
/// generator uses: replaying a recording means sending exactly what it
/// holds.
struct ReplayConfig {
    session: PathBuf,
    /// Where the recorded datagrams are re-sent. Loopback by default, since
    /// replay exists to drive an app on this machine.
    target: Ipv4Addr,
    /// Multiplier on the recorded timing; 2.0 replays an evening in half
    /// the time.
    speed: f64,
    repeat: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            devices: vec![
                SimulatedDevice {
                    device_number: 1,
                    name: "CDJ-2000".to_string(),
                },
                SimulatedDevice {
                    device_number: 2,
                    name: "CDJ-3000".to_string(),
                },
            ],
            interval: Duration::from_millis(1500),
            broadcast_addr: Ipv4Addr::new(255, 255, 255, 255),
            bpm: 128.0,
            precise_position: false,
            replay: None,
        }
    }
}

fn parse_args() -> Config {
    let defaults = Config::default();
    let mut interval = defaults.interval;
    let mut broadcast_addr = defaults.broadcast_addr;
    let mut bpm = defaults.bpm;
    let mut precise_position = defaults.precise_position;
    let mut replay_session: Option<PathBuf> = None;
    let mut replay_target = Ipv4Addr::LOCALHOST;
    let mut speed = 1.0f64;
    let mut repeat = false;
    let mut names: Option<Vec<String>> = None;
    let mut count: u8 = defaults.devices.len() as u8;
    let mut mixer = false;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--count" => {
                count = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .expect("--count needs a number");
            }
            "--names" => {
                names = args
                    .next()
                    .map(|v| v.split(',').map(str::to_string).collect());
            }
            "--mixer" => mixer = true,
            "--interval-ms" => {
                let ms: u64 = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .expect("--interval-ms needs a number");
                interval = Duration::from_millis(ms);
            }
            "--broadcast" => {
                broadcast_addr = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .expect("--broadcast needs an IPv4 address");
            }
            "--bpm" => {
                bpm = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .expect("--bpm needs a number");
            }
            "--precise-position" => precise_position = true,
            "--replay" => {
                replay_session = Some(PathBuf::from(
                    args.next().expect("--replay needs a session directory"),
                ));
            }
            "--replay-target" => {
                replay_target = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .expect("--replay-target needs an IPv4 address");
            }
            "--speed" => {
                speed = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .filter(|value: &f64| *value > 0.0)
                    .expect("--speed needs a positive number");
            }
            "--loop" => repeat = true,
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

    let mut devices: Vec<SimulatedDevice> = match names {
        Some(names) => names
            .into_iter()
            .enumerate()
            .map(|(index, name)| SimulatedDevice {
                device_number: index as u8 + 1,
                name,
            })
            .collect(),
        None => (1..=count)
            .map(|device_number| SimulatedDevice {
                device_number,
                name: format!("CDJ-3000 #{device_number}"),
            })
            .collect(),
    };
    if mixer {
        devices.push(SimulatedDevice {
            device_number: 33,
            name: "DJM-900NXS2".to_string(),
        });
    }

    let replay = replay_session.map(|session| ReplayConfig {
        session,
        target: replay_target,
        speed,
        repeat,
    });
    Config {
        devices,
        interval,
        broadcast_addr,
        bpm,
        precise_position,
        replay,
    }
}

fn print_usage() {
    eprintln!("cdj-sim — broadcasts fake Pro DJ Link traffic for testing without hardware");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --count N            number of simulated players (default 2)");
    eprintln!("  --names A,B,C        explicit player names, overrides --count");
    eprintln!("  --mixer              also simulate a mixer at device 33");
    eprintln!("  --interval-ms MS     keep-alive broadcast interval in ms (default 1500)");
    eprintln!("  --broadcast ADDR     broadcast address (default 255.255.255.255)");
    eprintln!("  --bpm BPM            shared tempo for every simulated deck (default 128)");
    eprintln!("  --precise-position   also emit CDJ-3000-style precise-position packets");
    eprintln!("                       for the first simulated player");
    eprintln!();
    eprintln!("Replay mode (ignores every option above):");
    eprintln!("  --replay DIR         replay a capture session's udp.jsonl as recorded");
    eprintln!("  --replay-target IP   where to send the replayed datagrams (default 127.0.0.1)");
    eprintln!("  --speed N            multiplier on the recorded timing (default 1.0)");
    eprintln!("  --loop               start the recording over when it ends");
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = parse_args();
    let peer_count = config.devices.len() as u8;
    let first_player = config
        .devices
        .iter()
        .find(|device| DeviceKind::from_device_number(device.device_number) == DeviceKind::Player)
        .map(|device| device.device_number);

    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).await?;

    if let Some(replay) = &config.replay {
        socket.set_broadcast(true)?;
        return replay_session(replay, &socket).await;
    }

    // The address our simulated players announce as their own. Receivers
    // read the IP out of the keep-alive payload (that is where real players
    // learn where to unicast status), so it has to be a real local address
    // rather than the wildcard this socket is bound to — loopback when the
    // sim is talking to something on this machine, which is what it is for.
    let sim_ip = local_ipv4().unwrap_or(Ipv4Addr::LOCALHOST);
    socket.set_broadcast(true)?;
    let socket = Arc::new(socket);

    println!(
        "Broadcasting {} simulated device(s) to {}:{{{},{},{}}} every {:?}",
        peer_count, config.broadcast_addr, ANNOUNCE_PORT, STATUS_PORT, BEAT_PORT, config.interval
    );
    for device in &config.devices {
        println!("  device {} — {}", device.device_number, device.name);
    }
    println!("Ctrl+C to stop.");

    for device in &config.devices {
        if DeviceKind::from_device_number(device.device_number) != DeviceKind::Player {
            continue; // The mixer's status packets don't share the CDJ shape.
        }
        let emit_precise_position =
            config.precise_position && first_player == Some(device.device_number);
        tokio::spawn(simulate_deck(
            device.device_number,
            device.device_number == first_player.unwrap_or(device.device_number),
            config.bpm,
            emit_precise_position,
            Arc::clone(&socket),
            config.broadcast_addr,
        ));
    }

    let mut tick = time::interval(config.interval);
    loop {
        tick.tick().await;
        for device in &config.devices {
            let mac = [0x02, 0x00, 0x00, 0x00, 0x00, device.device_number];
            let packet =
                build_keepalive(device.device_number, &device.name, mac, sim_ip, peer_count);
            if let Err(error) = socket
                .send_to(&packet, (config.broadcast_addr, ANNOUNCE_PORT))
                .await
            {
                eprintln!(
                    "failed to send keep-alive for device {}: {error}",
                    device.device_number
                );
            }
        }
    }
}

/// Sends a recorded session's datagrams back out with the spacing they were
/// recorded with, each to the port it arrived on.
///
/// Streamed rather than loaded: an evening's `udp.jsonl` is hundreds of
/// megabytes across close to a million records. Timing is anchored to the
/// first record and the wall clock at that moment, so a slow send cannot
/// make the whole replay drift later and later — a packet that comes due
/// while another is still going out is simply sent immediately, which is
/// also what a congested network does.
async fn replay_session(config: &ReplayConfig, socket: &UdpSocket) -> anyhow::Result<()> {
    let path = config.session.join(UDP_LOG);
    println!(
        "Replaying {} to {} at {}x",
        path.display(),
        config.target,
        config.speed
    );
    println!("Ctrl+C to stop.");

    loop {
        let records = stream_jsonl::<UdpRecord>(&path)?;
        let started_at = Instant::now();
        let mut first_record_micros: Option<i64> = None;
        let (mut sent, mut skipped) = (0u64, 0u64);

        for record in records {
            let record = match record {
                Ok(record) => record,
                Err(error) => {
                    // One unreadable line must not end a replay that may
                    // have hours left; the tally at the end reports it.
                    skipped += 1;
                    tracing::debug!(%error, "skipping an unreadable udp.jsonl record");
                    continue;
                }
            };
            let Some(payload) = record.payload() else {
                skipped += 1;
                continue;
            };

            let base = *first_record_micros.get_or_insert(record.t);
            let elapsed_micros = (record.t - base).max(0) as f64 / config.speed;
            time::sleep_until((started_at + Duration::from_micros(elapsed_micros as u64)).into())
                .await;

            if let Err(error) = socket.send_to(&payload, (config.target, record.port)).await {
                eprintln!(
                    "failed to replay a datagram to port {}: {error}",
                    record.port
                );
            }
            sent += 1;
            if sent % REPLAY_PROGRESS_EVERY == 0 {
                println!(
                    "  {sent} datagrams replayed ({:.0}s in)",
                    started_at.elapsed().as_secs_f64()
                );
            }
        }

        println!("Replayed {sent} datagrams ({skipped} unreadable records skipped)");
        if !config.repeat {
            return Ok(());
        }
    }
}

/// This machine's first non-loopback IPv4 address, for the keep-alives'
/// payload IP — mirrors `prolink-capture`'s `resolve_bind_ip`, kept
/// separate because that one belongs to a different binary target.
fn local_ipv4() -> Option<Ipv4Addr> {
    if_addrs::get_if_addrs()
        .ok()?
        .into_iter()
        .find_map(|interface| match interface.ip() {
            std::net::IpAddr::V4(ip) if !ip.is_loopback() => Some(ip),
            _ => None,
        })
}

/// Runs one simulated deck's status/beat traffic for as long as the process
/// lives: cued for [`CUE_DURATION`], then playing indefinitely at `bpm`,
/// broadcasting a status packet at the real ~5 Hz rate and a beat packet
/// once per simulated beat. `is_lead` makes this deck report itself as
/// tempo master and on-air, so a simulated multi-deck session has exactly
/// one of each — matching how a real booth usually runs.
async fn simulate_deck(
    device_number: u8,
    is_lead: bool,
    bpm: f64,
    emit_precise_position: bool,
    socket: Arc<UdpSocket>,
    broadcast_addr: Ipv4Addr,
) {
    let track_id = 1000 + u32::from(device_number);
    let started_at = Instant::now();
    let mut beat_count: u32 = 0;

    let mut status_tick = time::interval(STATUS_INTERVAL);
    let mut beat_tick = time::interval(Duration::from_secs_f64(60.0 / bpm));

    loop {
        tokio::select! {
            _ = status_tick.tick() => {
                let playing = started_at.elapsed() >= CUE_DURATION;
                let play_state = if playing { RawPlayState::Playing } else { RawPlayState::PausedAtCue };
                let beat_in_bar = (beat_count % 4) as u8 + 1;
                let packet = build_status(
                    device_number,
                    play_state,
                    is_lead,
                    !is_lead,
                    is_lead,
                    0.0,
                    Some(bpm),
                    Some(beat_count),
                    Some(beat_in_bar),
                    Some(track_id),
                );
                if let Err(error) = socket.send_to(&packet, (broadcast_addr, STATUS_PORT)).await {
                    eprintln!("failed to send status for device {device_number}: {error}");
                }

                if emit_precise_position && playing {
                    let position_ms = (started_at.elapsed() - CUE_DURATION).as_millis() as u32;
                    let packet = build_precise_position(device_number, position_ms);
                    if let Err(error) = socket.send_to(&packet, (broadcast_addr, BEAT_PORT)).await {
                        eprintln!("failed to send precise position for device {device_number}: {error}");
                    }
                }
            }
            _ = beat_tick.tick() => {
                if started_at.elapsed() < CUE_DURATION {
                    continue; // Not playing yet — a cued deck doesn't advance beats.
                }
                beat_count += 1;
                let beat_in_bar = (beat_count % 4) as u8 + 1;
                let packet = build_beat_packet(device_number, 0.0, Some(bpm), beat_in_bar);
                if let Err(error) = socket.send_to(&packet, (broadcast_addr, BEAT_PORT)).await {
                    eprintln!("failed to send beat for device {device_number}: {error}");
                }
            }
        }
    }
}
