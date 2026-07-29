//! Passive discovery of Pro DJ Link devices from their keep-alive broadcasts.
//!
//! A player or mixer that has finished its channel-number startup dance
//! settles into broadcasting a 54-byte keep-alive packet (type `0x06`) to
//! [`ANNOUNCE_PORT`] roughly every 1.5s for as long as it stays on the
//! network. [`DeviceWatcher`] listens for those and keeps a live list of
//! everything currently present — no virtual-CDJ startup sequence needed,
//! since we only observe, we never claim a channel ourselves.
//!
//! Byte offsets below come from Deep Symmetry's protocol analysis
//! (<https://djl-analysis.deepsymmetry.org/djl-analysis/startup.html>),
//! cross-checked against their `beat-link` Java library's
//! `DeviceAnnouncement` class, which parses the same packet in production.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use socket2::{Domain, Socket, Type};
use tokio::net::UdpSocket;
use tokio::sync::watch;
use ts_rs::TS;

use crate::{ANNOUNCE_PORT, PROLINK_HEADER};

/// Keep-alives arrive roughly every 1.5s; three missed beats is a safe
/// margin before a device is considered gone, matching the ~10s a real
/// player takes to drop a peer from its own count.
const DEVICE_TIMEOUT: Duration = Duration::from_secs(5);

/// How often the device list is swept for timed-out entries.
const SWEEP_INTERVAL: Duration = Duration::from_secs(1);

const KEEPALIVE_PACKET_TYPE: u8 = 0x06;
const KEEPALIVE_PACKET_LEN: usize = 0x36;
const NAME_OFFSET: usize = 0x0c;
const NAME_LEN: usize = 20;
const DEVICE_NUMBER_OFFSET: usize = 0x24;
const MAC_OFFSET: usize = 0x26;
const MAC_LEN: usize = 6;
const PEER_COUNT_OFFSET: usize = 0x30;

/// What a device number identifies, by Pro DJ Link convention — 1-6 are
/// players, 33 (`0x21`) is reserved for the mixer channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "DeviceKind.ts")]
pub enum DeviceKind {
    Player,
    Mixer,
    Other,
}

impl DeviceKind {
    fn from_device_number(device_number: u8) -> Self {
        match device_number {
            1..=6 => Self::Player,
            33 => Self::Mixer,
            _ => Self::Other,
        }
    }
}

/// One device currently visible on the Pro DJ Link network, as reconstructed
/// from its most recent keep-alive packet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "DiscoveredDevice.ts")]
pub struct DiscoveredDevice {
    pub name: String,
    pub device_number: u8,
    pub kind: DeviceKind,
    pub ip: String,
    pub mac_address: String,
    /// Peers this device reports seeing on the network, including itself.
    pub peer_count: u8,
}

/// Parses a keep-alive packet's payload; `None` if it isn't one — the
/// channel-claim packets a device sends while starting up share port and
/// header but not this shape, and should be ignored rather than misread.
fn parse_keepalive(data: &[u8], source_ip: Ipv4Addr) -> Option<DiscoveredDevice> {
    if data.len() != KEEPALIVE_PACKET_LEN
        || !data.starts_with(&PROLINK_HEADER)
        || data[PROLINK_HEADER.len()] != KEEPALIVE_PACKET_TYPE
    {
        return None;
    }

    let name = String::from_utf8_lossy(&data[NAME_OFFSET..NAME_OFFSET + NAME_LEN])
        .trim_end_matches('\0')
        .trim()
        .to_string();
    let device_number = data[DEVICE_NUMBER_OFFSET];
    let mac_address = data[MAC_OFFSET..MAC_OFFSET + MAC_LEN]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(":");
    let peer_count = data[PEER_COUNT_OFFSET];

    Some(DiscoveredDevice {
        name,
        device_number,
        kind: DeviceKind::from_device_number(device_number),
        ip: source_ip.to_string(),
        mac_address,
        peer_count,
    })
}

/// Listens for Pro DJ Link keep-alives and republishes the current device
/// list on every change — a device arriving, updating, or timing out.
///
/// Construction is synchronous and does no I/O; [`Self::run`] is the async
/// worker, spawned once a Tokio runtime is available — mirrors
/// `decktective_timecode::TimecodeService`.
pub struct DeviceWatcher {
    devices_tx: watch::Sender<Vec<DiscoveredDevice>>,
}

impl DeviceWatcher {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            devices_tx: watch::channel(Vec::new()).0,
        })
    }

    /// Subscribes to device-list changes. Pair with [`Self::devices`] to
    /// hydrate a fresh listener — changes published before this call are not
    /// replayed.
    pub fn subscribe(&self) -> watch::Receiver<Vec<DiscoveredDevice>> {
        self.devices_tx.subscribe()
    }

    pub fn devices(&self) -> Vec<DiscoveredDevice> {
        self.devices_tx.borrow().clone()
    }

    /// Runs until the socket errors out or the process exits. A bind failure
    /// (port already exclusively held, no permission, ...) is logged and
    /// leaves the device list permanently empty rather than crashing the
    /// app shell over a diagnostics panel.
    pub async fn run(self: Arc<Self>) {
        let socket = match bind_announce_socket() {
            Ok(socket) => socket,
            Err(error) => {
                tracing::error!(%error, "failed to bind the Pro DJ Link discovery socket");
                return;
            }
        };

        let mut seen: HashMap<u8, (DiscoveredDevice, Instant)> = HashMap::new();
        let mut buf = [0u8; 1500];
        let mut sweep = tokio::time::interval(SWEEP_INTERVAL);

        loop {
            tokio::select! {
                received = socket.recv_from(&mut buf) => {
                    let (len, source) = match received {
                        Ok(value) => value,
                        Err(error) => {
                            tracing::warn!(%error, "failed to read from the discovery socket");
                            continue;
                        }
                    };
                    let IpAddr::V4(source_ip) = source.ip() else { continue };
                    if let Some(device) = parse_keepalive(&buf[..len], source_ip) {
                        seen.insert(device.device_number, (device, Instant::now()));
                        self.publish(&seen);
                    }
                }
                _ = sweep.tick() => {
                    let before = seen.len();
                    seen.retain(|_, (_, last_seen)| last_seen.elapsed() < DEVICE_TIMEOUT);
                    if seen.len() != before {
                        self.publish(&seen);
                    }
                }
            }
        }
    }

    fn publish(&self, seen: &HashMap<u8, (DiscoveredDevice, Instant)>) {
        let mut devices: Vec<DiscoveredDevice> =
            seen.values().map(|(device, _)| device.clone()).collect();
        devices.sort_by_key(|device| device.device_number);
        // No subscribers yet (window still opening) is not an error.
        let _ = self.devices_tx.send(devices);
    }
}

/// Binds with `SO_REUSEADDR` (and, on Unix, `SO_REUSEPORT`) so this
/// diagnostics listener can coexist with rekordbox or another Pro DJ Link
/// tool already bound to the same announce port on this machine.
fn bind_announce_socket() -> anyhow::Result<UdpSocket> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, None)?;
    socket.set_reuse_address(true)?;
    #[cfg(unix)]
    socket.set_reuse_port(true)?;
    socket.set_broadcast(true)?;
    socket.set_nonblocking(true)?;
    socket.bind(&SocketAddr::from((Ipv4Addr::UNSPECIFIED, ANNOUNCE_PORT)).into())?;
    Ok(UdpSocket::from_std(socket.into())?)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a well-formed 54-byte keep-alive payload for a player with the
    /// given number, name and MAC — the IP travels separately, as it does
    /// over the wire (the sender's socket address, not a payload field).
    fn keepalive_bytes(device_number: u8, name: &str, mac: [u8; 6]) -> Vec<u8> {
        let mut packet = vec![0u8; KEEPALIVE_PACKET_LEN];
        packet[..PROLINK_HEADER.len()].copy_from_slice(&PROLINK_HEADER);
        packet[PROLINK_HEADER.len()] = KEEPALIVE_PACKET_TYPE;
        let name_bytes = name.as_bytes();
        packet[NAME_OFFSET..NAME_OFFSET + name_bytes.len()].copy_from_slice(name_bytes);
        packet[DEVICE_NUMBER_OFFSET] = device_number;
        packet[MAC_OFFSET..MAC_OFFSET + MAC_LEN].copy_from_slice(&mac);
        packet[PEER_COUNT_OFFSET] = 1;
        packet
    }

    #[test]
    fn parses_a_player_keepalive() {
        let bytes = keepalive_bytes(2, "CDJ-3000", [0x00, 0x11, 0x22, 0x33, 0x44, 0x55]);
        let device = parse_keepalive(&bytes, Ipv4Addr::new(192, 168, 1, 42)).expect("should parse");

        assert_eq!(device.name, "CDJ-3000");
        assert_eq!(device.device_number, 2);
        assert_eq!(device.kind, DeviceKind::Player);
        assert_eq!(device.ip, "192.168.1.42");
        assert_eq!(device.mac_address, "00:11:22:33:44:55");
        assert_eq!(device.peer_count, 1);
    }

    #[test]
    fn device_number_33_is_the_mixer() {
        let bytes = keepalive_bytes(33, "DJM-900NXS2", [0; 6]);
        let device = parse_keepalive(&bytes, Ipv4Addr::LOCALHOST).expect("should parse");
        assert_eq!(device.kind, DeviceKind::Mixer);
    }

    #[test]
    fn rejects_the_wrong_length() {
        let mut bytes = keepalive_bytes(2, "CDJ-3000", [0; 6]);
        bytes.push(0);
        assert!(parse_keepalive(&bytes, Ipv4Addr::LOCALHOST).is_none());
    }

    #[test]
    fn rejects_a_non_keepalive_packet_type() {
        let mut bytes = keepalive_bytes(2, "CDJ-3000", [0; 6]);
        bytes[PROLINK_HEADER.len()] = 0x0a; // Initial device announcement, not a keep-alive.
        assert!(parse_keepalive(&bytes, Ipv4Addr::LOCALHOST).is_none());
    }

    #[test]
    fn rejects_a_foreign_header() {
        let mut bytes = keepalive_bytes(2, "CDJ-3000", [0; 6]);
        bytes[0] = 0x00;
        assert!(parse_keepalive(&bytes, Ipv4Addr::LOCALHOST).is_none());
    }
}
