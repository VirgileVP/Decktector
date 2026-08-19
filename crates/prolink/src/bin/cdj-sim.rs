//! Standalone dev tool that broadcasts fake Pro DJ Link traffic — keep
//! -alives, status packets and beat packets — so decktective's real engine
//! (`prolink-engine::ProlinkEngine`) can be exercised against something that
//! actually speaks the wire protocol, without real CDJs on the network.
//! Unlike `decktective_core::sim::SimEngine`, which bypasses the protocol
//! entirely, this makes `cargo run -p decktective` (with `DECKTECTIVE_SIM`
//! unset) show live decks end to end through the real parsing path. Run it
//! on the same machine as decktective — no virtual-CDJ startup dance, just
//! the same broadcasts a real player settles into once it's up.
//!
//! Usage:
//!   cdj-sim [--count N] [--names CDJ-2000,CDJ-3000] [--mixer]
//!           [--interval-ms 1500] [--broadcast 255.255.255.255]
//!           [--bpm 128] [--precise-position]

use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use prolink::beat::{build_beat_packet, build_precise_position};
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
}

impl Default for Config {
    fn default() -> Self {
        Self {
            devices: vec![
                SimulatedDevice { device_number: 1, name: "CDJ-2000".to_string() },
                SimulatedDevice { device_number: 2, name: "CDJ-3000".to_string() },
            ],
            interval: Duration::from_millis(1500),
            broadcast_addr: Ipv4Addr::new(255, 255, 255, 255),
            bpm: 128.0,
            precise_position: false,
        }
    }
}

fn parse_args() -> Config {
    let defaults = Config::default();
    let mut interval = defaults.interval;
    let mut broadcast_addr = defaults.broadcast_addr;
    let mut bpm = defaults.bpm;
    let mut precise_position = defaults.precise_position;
    let mut names: Option<Vec<String>> = None;
    let mut count: u8 = defaults.devices.len() as u8;
    let mut mixer = false;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--count" => {
                count = args.next().and_then(|v| v.parse().ok()).expect("--count needs a number");
            }
            "--names" => {
                names = args.next().map(|v| v.split(',').map(str::to_string).collect());
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
                bpm = args.next().and_then(|v| v.parse().ok()).expect("--bpm needs a number");
            }
            "--precise-position" => precise_position = true,
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
            .map(|(index, name)| SimulatedDevice { device_number: index as u8 + 1, name })
            .collect(),
        None => (1..=count)
            .map(|device_number| SimulatedDevice {
                device_number,
                name: format!("CDJ-3000 #{device_number}"),
            })
            .collect(),
    };
    if mixer {
        devices.push(SimulatedDevice { device_number: 33, name: "DJM-900NXS2".to_string() });
    }

    Config { devices, interval, broadcast_addr, bpm, precise_position }
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
        let emit_precise_position = config.precise_position && first_player == Some(device.device_number);
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
            let packet = build_keepalive(device.device_number, &device.name, mac, sim_ip, peer_count);
            if let Err(error) = socket.send_to(&packet, (config.broadcast_addr, ANNOUNCE_PORT)).await {
                eprintln!("failed to send keep-alive for device {}: {error}", device.device_number);
            }
        }
    }
}

/// This machine's first non-loopback IPv4 address, for the keep-alives'
/// payload IP — mirrors `prolink-capture`'s `resolve_bind_ip`, kept
/// separate because that one belongs to a different binary target.
fn local_ipv4() -> Option<Ipv4Addr> {
    if_addrs::get_if_addrs().ok()?.into_iter().find_map(|interface| match interface.ip() {
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
