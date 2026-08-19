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
use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use ts_rs::TS;

use crate::{bind_udp_socket, ANNOUNCE_PORT, PROLINK_HEADER};

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
const IP_OFFSET: usize = 0x2c;
const IP_LEN: usize = 4;
const PEER_COUNT_OFFSET: usize = 0x30;
/// The fields a real keep-alive carries besides name/number/MAC/IP/peer
/// count, at the offsets a captured CDJ-3000 and DJM-900NXS2 agree on
/// (`capture/` sessions, cross-checked against Deep Symmetry's analysis):
/// `0x20`-`0x21` a constant plus the device kind, `0x22`-`0x23` the packet
/// length, `0x25` the device kind again, `0x34`-`0x35` a trailing constant
/// pair that also varies by kind. They are transmitted, not just parsed:
/// players decide whether to treat us as a peer worth unicasting status to
/// from the whole packet, so a keep-alive with these left zeroed is a
/// keep-alive real hardware can ignore.
const PROTOCOL_OFFSET: usize = 0x20;
const KIND_OFFSET: usize = 0x21;
const LENGTH_OFFSET: usize = 0x22;
const KIND_REPEAT_OFFSET: usize = 0x25;
const TRAILER_OFFSET: usize = 0x34;
/// The `0x21`/`0x25` device-kind value a CDJ sends (a mixer sends `0x02`
/// there); we announce ourselves as a player because that is the identity
/// metadata queries are answered for.
const KIND_PLAYER: u8 = 0x03;

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
    /// Public so other packet types on the same network (status, beat —
    /// which share the same device-number convention but aren't
    /// keep-alives) can classify a device number without duplicating this
    /// match.
    pub fn from_device_number(device_number: u8) -> Self {
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
    seen: Mutex<HashMap<u8, (DiscoveredDevice, Instant)>>,
}

impl DeviceWatcher {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            devices_tx: watch::channel(Vec::new()).0,
            seen: Mutex::new(HashMap::new()),
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

    /// Feeds one received datagram to the watcher, publishing an updated
    /// device list if it turned out to be a keep-alive. Returns whether it
    /// was one.
    ///
    /// Public so a caller that already has its own listener on
    /// [`ANNOUNCE_PORT`] can drive discovery from it instead of having this
    /// watcher open a second socket on the same port — which does not work
    /// on Windows, where only `SO_REUSEADDR` is available and a datagram is
    /// delivered to just one of the bound sockets (`SO_REUSEPORT`, which
    /// would fan it out to all of them, is Unix-only — see
    /// [`crate::bind_udp_socket`]). `prolink-capture` relies on this.
    ///
    /// Pair with [`Self::sweep`]: a watcher driven this way still needs
    /// something to expire devices that stopped broadcasting.
    pub fn ingest(&self, data: &[u8], source_ip: Ipv4Addr) -> bool {
        let Some(device) = parse_keepalive(data, source_ip) else {
            return false;
        };
        let mut seen = self.seen();
        seen.insert(device.device_number, (device, Instant::now()));
        let snapshot = Self::snapshot(&seen);
        drop(seen);
        self.publish(snapshot);
        true
    }

    /// Drops devices whose last keep-alive is older than [`DEVICE_TIMEOUT`],
    /// publishing only if that changed the list. Call every
    /// [`Self::SWEEP_INTERVAL`] when driving the watcher through
    /// [`Self::ingest`]; [`Self::run`] does it internally.
    pub fn sweep(&self) {
        let mut seen = self.seen();
        let before = seen.len();
        seen.retain(|_, (_, last_seen)| last_seen.elapsed() < DEVICE_TIMEOUT);
        if seen.len() == before {
            return;
        }
        let snapshot = Self::snapshot(&seen);
        drop(seen);
        self.publish(snapshot);
    }

    /// How often a watcher driven through [`Self::ingest`] should call
    /// [`Self::sweep`] — the same cadence [`Self::run`] uses internally.
    pub const SWEEP_INTERVAL: Duration = SWEEP_INTERVAL;

    /// Runs until the socket errors out or the process exits, owning its own
    /// listener on [`ANNOUNCE_PORT`]. A bind failure (port already
    /// exclusively held, no permission, ...) is logged and leaves the device
    /// list permanently empty rather than crashing the app shell over a
    /// diagnostics panel.
    ///
    /// Only for callers with no listener of their own on that port; anything
    /// already reading it must use [`Self::ingest`] plus [`Self::sweep`]
    /// instead, for the delivery reason spelled out on `ingest`.
    pub async fn run(self: Arc<Self>) {
        let socket = match bind_udp_socket(ANNOUNCE_PORT) {
            Ok(socket) => socket,
            Err(error) => {
                tracing::error!(%error, "failed to bind the Pro DJ Link discovery socket");
                return;
            }
        };

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
                    self.ingest(&buf[..len], source_ip);
                }
                _ = sweep.tick() => self.sweep(),
            }
        }
    }

    /// The `seen` map, recovering from a poisoned lock rather than
    /// propagating the panic: a capture session losing its device table
    /// mid-set is worse than carrying on with whatever is in the map.
    fn seen(&self) -> std::sync::MutexGuard<'_, HashMap<u8, (DiscoveredDevice, Instant)>> {
        self.seen.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn snapshot(seen: &HashMap<u8, (DiscoveredDevice, Instant)>) -> Vec<DiscoveredDevice> {
        let mut devices: Vec<DiscoveredDevice> =
            seen.values().map(|(device, _)| device.clone()).collect();
        devices.sort_by_key(|device| device.device_number);
        devices
    }

    fn publish(&self, devices: Vec<DiscoveredDevice>) {
        // No subscribers yet (window still opening) is not an error.
        let _ = self.devices_tx.send(devices);
    }
}

/// Builds a well-formed 54-byte keep-alive payload for a player with the
/// given number, name, MAC, IP and peer count, byte-for-byte in the shape a
/// real CDJ-3000 broadcasts (see [`KIND_OFFSET`]'s doc comment for the
/// fields beyond the obvious ones).
///
/// `ip` is a payload field, not just the sender's socket address: players
/// unicast their status packets to the address they read out of a peer's
/// keep-alive, so a keep-alive announcing `0.0.0.0` gets a device listed on
/// other tools' device tables and nothing on [`crate::STATUS_PORT`] — which
/// is exactly what the first hardware capture recorded.
///
/// Encoding lives next to [`parse_keepalive`] so the wire format is defined
/// once; used by [`crate::presence`], by the `prolink-capture` tool and by
/// the `cdj-sim` dev tool that simulates fake devices on the network.
pub fn build_keepalive(device_number: u8, name: &str, mac: [u8; 6], ip: Ipv4Addr, peer_count: u8) -> Vec<u8> {
    let mut packet = vec![0u8; KEEPALIVE_PACKET_LEN];
    packet[..PROLINK_HEADER.len()].copy_from_slice(&PROLINK_HEADER);
    packet[PROLINK_HEADER.len()] = KEEPALIVE_PACKET_TYPE;
    let name_bytes = name.as_bytes();
    let name_len = name_bytes.len().min(NAME_LEN);
    packet[NAME_OFFSET..NAME_OFFSET + name_len].copy_from_slice(&name_bytes[..name_len]);
    packet[PROTOCOL_OFFSET] = 0x01;
    packet[KIND_OFFSET] = KIND_PLAYER;
    packet[LENGTH_OFFSET..LENGTH_OFFSET + 2].copy_from_slice(&(KEEPALIVE_PACKET_LEN as u16).to_be_bytes());
    packet[DEVICE_NUMBER_OFFSET] = device_number;
    packet[KIND_REPEAT_OFFSET] = 0x01;
    packet[MAC_OFFSET..MAC_OFFSET + MAC_LEN].copy_from_slice(&mac);
    packet[IP_OFFSET..IP_OFFSET + IP_LEN].copy_from_slice(&ip.octets());
    packet[PEER_COUNT_OFFSET] = peer_count;
    packet[TRAILER_OFFSET] = 0x01;
    packet[TRAILER_OFFSET + 1] = 0x64;
    packet
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keepalive_bytes(device_number: u8, name: &str, mac: [u8; 6]) -> Vec<u8> {
        build_keepalive(device_number, name, mac, Ipv4Addr::new(192, 168, 1, 42), 1)
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

    /// The exact keep-alive a CDJ-3000 broadcast during the 2026-08-19
    /// hardware session (`capture/capture-20260819-083643/udp.jsonl`), with
    /// only the name/number/MAC/IP that identify *that* player differing —
    /// every other byte we transmit has to match hardware, since the whole
    /// packet is what decides whether real players start unicasting status
    /// to us.
    #[test]
    fn matches_a_captured_cdj_3000_keepalive() {
        let captured = "5173707431576d4a4f4c060043444a2d33303030000000000000000000000000010300360301c83dfc126917a9fe0583020000000164";
        let bytes = build_keepalive(
            3,
            "CDJ-3000",
            [0xc8, 0x3d, 0xfc, 0x12, 0x69, 0x17],
            Ipv4Addr::new(169, 254, 5, 131),
            2,
        );
        let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        assert_eq!(hex, captured);
    }

    #[test]
    fn rejects_a_foreign_header() {
        let mut bytes = keepalive_bytes(2, "CDJ-3000", [0; 6]);
        bytes[0] = 0x00;
        assert!(parse_keepalive(&bytes, Ipv4Addr::LOCALHOST).is_none());
    }
}
