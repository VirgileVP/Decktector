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

use std::collections::{BTreeSet, HashMap};
use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use ts_rs::TS;

use crate::{bind_udp_socket, ANNOUNCE_PORT, PROLINK_HEADER};

/// Keep-alives arrive roughly every 1.5s; ~15s allows for network jitter
/// and transient drops without false disconnects, matching real CDJs.
const DEVICE_TIMEOUT: Duration = Duration::from_secs(15);

/// How often the device list is swept for timed-out entries.
const SWEEP_INTERVAL: Duration = Duration::from_secs(1);

const KEEPALIVE_PACKET_TYPE: u8 = 0x06;
const KEEPALIVE_PACKET_LEN: usize = 0x36;
const NAME_OFFSET: usize = 0x0c;
const NAME_LEN: usize = 20;
const DEVICE_NUMBER_OFFSET: usize = 0x24;
const MAC_OFFSET: usize = 0x26;
const MAC_LEN: usize = 6;
/// What [`DiscoveredDevice::mac_address`] renders an all-zero MAC as — the
/// value a device would announce if it had no usable hardware address.
const UNSET_MAC_ADDRESS: &str = "00:00:00:00:00:00";
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
            1..=7 => Self::Player,
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
    /// How many devices currently announce this `device_number`, this one
    /// included. `1` on a correctly configured network; above that, two
    /// devices share a number — typically two mixers, which both announce
    /// the reserved mixer number 33 (a DJM-A9 next to a DJM-V10, say), or
    /// two players set to the same channel. Filled in by [`DeviceWatcher`]
    /// when it publishes the list, so everything downstream (the engine, the
    /// Devices tab) reads one answer instead of recounting.
    pub number_holders: u8,
}

impl DiscoveredDevice {
    /// Whether another device announces the same device number as this one.
    pub fn has_number_conflict(&self) -> bool {
        self.number_holders > 1
    }
}

/// The device numbers announced by more than one device in `devices`, in
/// ascending order. For callers that react to the conflict itself (logging
/// it once) rather than rendering each affected device.
pub fn conflicting_numbers(devices: &[DiscoveredDevice]) -> BTreeSet<u8> {
    devices
        .iter()
        .filter(|device| device.has_number_conflict())
        .map(|device| device.device_number)
        .collect()
}

/// Sets every device's [`DiscoveredDevice::number_holders`] from how many
/// entries of `devices` announce its number.
fn count_number_holders(devices: &mut [DiscoveredDevice]) {
    let mut holders_by_number: HashMap<u8, usize> = HashMap::new();
    for device in devices.iter() {
        *holders_by_number.entry(device.device_number).or_default() += 1;
    }
    for device in devices.iter_mut() {
        let holders = holders_by_number[&device.device_number];
        device.number_holders = u8::try_from(holders).unwrap_or(u8::MAX);
    }
}

/// What makes two keep-alives the same physical device, as opposed to the
/// same device number.
///
/// The device number is *not* an identity: every mixer announces the
/// reserved number 33, so two mixers on one network (a DJM-A9 and a DJM-V10
/// at the same gig) share it, and two players can be misconfigured onto one
/// number. Keyed by number they overwrite each other and the list flaps
/// between them with every keep-alive. The hardware address is what stays
/// put when a device is renumbered, so it is the key — a player that
/// changes its number re-announces under the same MAC and replaces its old
/// entry instead of leaving a ghost behind until [`DEVICE_TIMEOUT`].
///
/// A device announcing an all-zero MAC has no identity of its own (several
/// such devices would collapse into one entry), so it falls back to its
/// source IP — unique per host on a network, though it forgoes the
/// renumber-in-place behaviour if the device's address also changes. Not
/// observed on hardware: every captured CDJ and DJM announced a real MAC;
/// this only guards against a device or simulator that does not.
///
/// Unverified on hardware: that two DJMs really announce the same number 33
/// on one network, which is what this module is built to survive.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum DeviceIdentity {
    Mac(String),
    Address(String),
}

impl DeviceIdentity {
    fn of(device: &DiscoveredDevice) -> Self {
        if device.mac_address == UNSET_MAC_ADDRESS {
            Self::Address(device.ip.clone())
        } else {
            Self::Mac(device.mac_address.clone())
        }
    }
}

/// One entry per physical device, with when its last keep-alive arrived.
type SeenDevices = HashMap<DeviceIdentity, (DiscoveredDevice, Instant)>;

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
        number_holders: 1,
    })
}

/// Listens for Pro DJ Link keep-alives and republishes the current device
/// list on every change — a device arriving, updating, or timing out.
///
/// Each physical device is its own entry (see [`DeviceIdentity`]), so devices
/// sharing a number all appear, flagged through
/// [`DiscoveredDevice::number_holders`]. The published list is ordered by
/// device number, then MAC address, so it does not reshuffle between
/// snapshots.
///
/// Construction is synchronous and does no I/O; [`Self::run`] is the async
/// worker, spawned once a Tokio runtime is available — mirrors
/// `decktector_timecode::TimecodeService`.
pub struct DeviceWatcher {
    devices_tx: watch::Sender<Vec<DiscoveredDevice>>,
    seen: Mutex<SeenDevices>,
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
        seen.insert(DeviceIdentity::of(&device), (device, Instant::now()));
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
    fn seen(&self) -> std::sync::MutexGuard<'_, SeenDevices> {
        self.seen
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn snapshot(seen: &SeenDevices) -> Vec<DiscoveredDevice> {
        let mut devices: Vec<DiscoveredDevice> =
            seen.values().map(|(device, _)| device.clone()).collect();
        devices.sort_by(|a, b| {
            (a.device_number, &a.mac_address, &a.ip).cmp(&(b.device_number, &b.mac_address, &b.ip))
        });
        count_number_holders(&mut devices);
        devices
    }

    fn publish(&self, devices: Vec<DiscoveredDevice>) {
        // `send_replace` stores the list even with no subscribers (window
        // still opening), where `send` would drop it and leave
        // [`Self::devices`] stale for a listener hydrating late.
        self.devices_tx.send_replace(devices);
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
pub fn build_keepalive(
    device_number: u8,
    name: &str,
    mac: [u8; 6],
    ip: Ipv4Addr,
    peer_count: u8,
) -> Vec<u8> {
    let mut packet = vec![0u8; KEEPALIVE_PACKET_LEN];
    packet[..PROLINK_HEADER.len()].copy_from_slice(&PROLINK_HEADER);
    packet[PROLINK_HEADER.len()] = KEEPALIVE_PACKET_TYPE;
    let name_bytes = name.as_bytes();
    let name_len = name_bytes.len().min(NAME_LEN);
    packet[NAME_OFFSET..NAME_OFFSET + name_len].copy_from_slice(&name_bytes[..name_len]);
    packet[PROTOCOL_OFFSET] = 0x01;
    packet[KIND_OFFSET] = KIND_PLAYER;
    packet[LENGTH_OFFSET..LENGTH_OFFSET + 2]
        .copy_from_slice(&(KEEPALIVE_PACKET_LEN as u16).to_be_bytes());
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

    const MIXER_A_MAC: [u8; 6] = [0x00, 0x11, 0x22, 0x33, 0x44, 0xa9];
    const MIXER_B_MAC: [u8; 6] = [0x00, 0x11, 0x22, 0x33, 0x44, 0xb0];
    const PLAYER_MAC: [u8; 6] = [0xc8, 0x3d, 0xfc, 0x12, 0x69, 0x17];

    fn ip(last_octet: u8) -> Ipv4Addr {
        Ipv4Addr::new(192, 168, 1, last_octet)
    }

    /// Ages a device's last keep-alive so [`DeviceWatcher::sweep`] sees it as
    /// older than [`DEVICE_TIMEOUT`], without sleeping through the timeout.
    fn age_past_timeout(watcher: &DeviceWatcher, mac_address: &str) {
        let stale = Instant::now()
            .checked_sub(DEVICE_TIMEOUT + Duration::from_secs(1))
            .expect("the monotonic clock is older than the device timeout");
        let mut seen = watcher.seen();
        let (_, last_seen) = seen
            .get_mut(&DeviceIdentity::Mac(mac_address.to_string()))
            .expect("the device should be tracked");
        *last_seen = stale;
    }

    #[test]
    fn two_mixers_on_number_33_are_two_devices() {
        let watcher = DeviceWatcher::new();
        watcher.ingest(&keepalive_bytes(33, "DJM-A9", MIXER_A_MAC), ip(10));
        watcher.ingest(&keepalive_bytes(33, "DJM-V10", MIXER_B_MAC), ip(11));

        let devices = watcher.devices();
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].name, "DJM-A9");
        assert_eq!(devices[0].ip, "192.168.1.10");
        assert_eq!(devices[1].name, "DJM-V10");
        assert_eq!(devices[1].ip, "192.168.1.11");
    }

    #[test]
    fn a_repeated_keepalive_does_not_flip_between_mixers() {
        let watcher = DeviceWatcher::new();
        for _ in 0..3 {
            watcher.ingest(&keepalive_bytes(33, "DJM-A9", MIXER_A_MAC), ip(10));
            watcher.ingest(&keepalive_bytes(33, "DJM-V10", MIXER_B_MAC), ip(11));
        }

        let names: Vec<String> = watcher.devices().into_iter().map(|d| d.name).collect();
        assert_eq!(names, ["DJM-A9", "DJM-V10"]);
    }

    #[test]
    fn the_published_list_does_not_depend_on_arrival_order() {
        let forward = DeviceWatcher::new();
        forward.ingest(&keepalive_bytes(33, "DJM-A9", MIXER_A_MAC), ip(10));
        forward.ingest(&keepalive_bytes(33, "DJM-V10", MIXER_B_MAC), ip(11));
        let reverse = DeviceWatcher::new();
        reverse.ingest(&keepalive_bytes(33, "DJM-V10", MIXER_B_MAC), ip(11));
        reverse.ingest(&keepalive_bytes(33, "DJM-A9", MIXER_A_MAC), ip(10));

        assert_eq!(forward.devices(), reverse.devices());
    }

    #[test]
    fn a_renumbered_device_replaces_its_old_entry() {
        let watcher = DeviceWatcher::new();
        watcher.ingest(&keepalive_bytes(2, "CDJ-3000", PLAYER_MAC), ip(20));
        watcher.ingest(&keepalive_bytes(3, "CDJ-3000", PLAYER_MAC), ip(20));

        let devices = watcher.devices();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].device_number, 3);
    }

    #[test]
    fn a_renumber_onto_a_taken_number_leaves_exactly_two_devices() {
        let watcher = DeviceWatcher::new();
        watcher.ingest(&keepalive_bytes(1, "CDJ-3000", PLAYER_MAC), ip(20));
        watcher.ingest(&keepalive_bytes(2, "CDJ-2000NXS2", [0xaa; 6]), ip(21));
        watcher.ingest(&keepalive_bytes(2, "CDJ-3000", PLAYER_MAC), ip(20));

        let numbers: Vec<u8> = watcher.devices().iter().map(|d| d.device_number).collect();
        assert_eq!(numbers, [2, 2]);
    }

    #[test]
    fn devices_without_a_mac_are_told_apart_by_ip() {
        let watcher = DeviceWatcher::new();
        watcher.ingest(&keepalive_bytes(33, "DJM-A9", [0; 6]), ip(10));
        watcher.ingest(&keepalive_bytes(33, "DJM-V10", [0; 6]), ip(11));
        // A repeat from the first must update it, not add a third entry.
        watcher.ingest(&keepalive_bytes(33, "DJM-A9", [0; 6]), ip(10));

        assert_eq!(watcher.devices().len(), 2);
    }

    #[test]
    fn sweep_expires_only_the_device_that_went_silent() {
        let watcher = DeviceWatcher::new();
        watcher.ingest(&keepalive_bytes(33, "DJM-A9", MIXER_A_MAC), ip(10));
        watcher.ingest(&keepalive_bytes(33, "DJM-V10", MIXER_B_MAC), ip(11));

        age_past_timeout(&watcher, "00:11:22:33:44:a9");
        watcher.sweep();

        let devices = watcher.devices();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].name, "DJM-V10");
        assert!(!devices[0].has_number_conflict());
    }

    #[test]
    fn sweep_keeps_devices_that_are_still_broadcasting() {
        let watcher = DeviceWatcher::new();
        watcher.ingest(&keepalive_bytes(33, "DJM-A9", MIXER_A_MAC), ip(10));
        watcher.sweep();
        assert_eq!(watcher.devices().len(), 1);
    }

    #[test]
    fn devices_sharing_a_number_are_flagged() {
        let watcher = DeviceWatcher::new();
        watcher.ingest(&keepalive_bytes(33, "DJM-A9", MIXER_A_MAC), ip(10));
        watcher.ingest(&keepalive_bytes(33, "DJM-V10", MIXER_B_MAC), ip(11));
        watcher.ingest(&keepalive_bytes(1, "CDJ-3000", PLAYER_MAC), ip(20));

        let devices = watcher.devices();
        let player = &devices[0];
        assert!(!player.has_number_conflict());
        assert_eq!(player.number_holders, 1);
        for mixer in &devices[1..] {
            assert!(mixer.has_number_conflict());
            assert_eq!(mixer.number_holders, 2);
        }
        assert_eq!(conflicting_numbers(&devices), BTreeSet::from([33]));
    }

    #[test]
    fn a_lone_device_is_not_a_conflict() {
        let watcher = DeviceWatcher::new();
        watcher.ingest(&keepalive_bytes(33, "DJM-900NXS2", MIXER_A_MAC), ip(10));

        let devices = watcher.devices();
        assert!(!devices[0].has_number_conflict());
        assert!(conflicting_numbers(&devices).is_empty());
    }

    #[test]
    fn two_players_on_one_number_are_a_conflict_too() {
        let watcher = DeviceWatcher::new();
        watcher.ingest(&keepalive_bytes(2, "CDJ-3000", PLAYER_MAC), ip(20));
        watcher.ingest(&keepalive_bytes(2, "CDJ-3000", [0xaa; 6]), ip(21));

        assert_eq!(conflicting_numbers(&watcher.devices()), BTreeSet::from([2]));
    }

    #[test]
    fn the_conflict_clears_once_one_of_the_devices_renumbers() {
        let watcher = DeviceWatcher::new();
        watcher.ingest(&keepalive_bytes(2, "CDJ-3000", PLAYER_MAC), ip(20));
        watcher.ingest(&keepalive_bytes(2, "CDJ-3000", [0xaa; 6]), ip(21));
        watcher.ingest(&keepalive_bytes(3, "CDJ-3000", [0xaa; 6]), ip(21));

        assert!(conflicting_numbers(&watcher.devices()).is_empty());
    }
}
