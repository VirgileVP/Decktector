//! Claims a virtual-CDJ identity for Decktective on the Pro DJ Link network.
//!
//! Deep Symmetry's analysis of the startup handshake
//! (<https://djl-analysis.deepsymmetry.org/djl-analysis/startup.html>)
//! documents four claim stages plus a defend packet a device sends when
//! another one tries to claim a number it already holds — but leaves several
//! of the byte offsets those claim-stage packets need (MAC placement in the
//! first-stage packet, the exact contested-number offset in the defend
//! packet) undocumented precisely enough to transmit safely onto a real
//! network: a malformed claim packet risks confusing real players mid-claim,
//! which is a worse failure than not claiming at all. The same page also
//! confirms passive listening works without claiming anything, with the one
//! limitation being some hardware (XDJ-XZ) that omits unsolicited media-slot
//! info from a non-participant — a metadata-phase concern, not this one.
//!
//! So for now: no active handshake. This module picks a free number by
//! watching [`crate::discovery::DeviceWatcher`]'s live list (the same
//! passive, already-correct read path used elsewhere) and broadcasts the
//! settled-state keep-alive real players use once they're done claiming —
//! [`crate::discovery::build_keepalive`], already byte-verified by its own
//! tests. This is enough for decktective to show up as a peer in its own
//! diagnostics panel and to have an identity for later phases' metadata
//! queries, without ever transmitting a claim-stage packet this
//! implementation can't fully verify. If it turns out real hardware needs
//! the full handshake to answer status/beat traffic too, that's the next
//! thing to build here — see the phase-2 plan's open question about this.
//!
//! The 2026-08-19 hardware session answered the other half of that open
//! question: keep-alives alone *are* enough. Two CDJ-3000s and a
//! DJM-900NXS2 broadcast discovery and beat packets at a purely passive
//! listener but sent nothing at all on [`crate::STATUS_PORT`], because
//! players unicast status only to peers whose keep-alives they have seen
//! (<https://djl-analysis.deepsymmetry.org/djl-analysis/vcdj.html>). So
//! this module is not a nicety for the diagnostics panel — no presence
//! means no status, and no status means no metadata either, since every
//! later phase keys off the currently-loaded track.

use std::net::Ipv4Addr;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use tokio::net::UdpSocket;
use tokio::sync::watch;

use crate::discovery::{build_keepalive, DeviceWatcher, DiscoveredDevice};
use crate::ANNOUNCE_PORT;

/// Matches real players' own keep-alive cadence (see
/// [`crate::discovery`]'s module doc).
const KEEPALIVE_INTERVAL: Duration = Duration::from_millis(1500);

/// Decktective's default virtual player number — chosen to stay clear of the
/// XDJ-XZ's self-claimed 1+2 while remaining in the 1-6 range CDJs accept
/// database queries from (see the project's README).
const PREFERRED_DEVICE_NUMBER: u8 = 5;

/// Fallback order when the preferred number is taken: try 6 next (still
/// outside the 1-4 range four-CDJ setups fill), then fall back through 1-4
/// as a last resort for larger/unusual networks.
const CANDIDATE_DEVICE_NUMBERS: [u8; 6] = [PREFERRED_DEVICE_NUMBER, 6, 1, 2, 3, 4];

const BROADCAST_ADDR: Ipv4Addr = Ipv4Addr::new(255, 255, 255, 255);

/// The MAC our keep-alives actually went out with, once [`Presence::run`]
/// has resolved it. Process-global because it is a process-global fact —
/// there is one Pro DJ Link identity per process — and because
/// [`is_own_broadcast`] is called from code that has no handle on
/// [`Presence`] (`prolink_engine::engine`'s deck mapping).
static ANNOUNCED_MAC: OnceLock<[u8; 6]> = OnceLock::new();

/// Fallback MAC for our virtual identity when the real adapter's can't be
/// read: locally-administered, keyed by device number so it's deterministic
/// after a renumber and recognizable as "us" (not a real Pioneer OUI) in a
/// packet capture.
///
/// A fallback, not a preference. Real players appear to send status to the
/// MAC a peer announced rather than ARPing for the IP, so a made-up MAC
/// gets us counted as a peer and sent nothing — see [`interface_mac`].
fn presence_mac(device_number: u8) -> [u8; 6] {
    [0x02, 0x00, 0x00, 0x00, 0x00, device_number]
}

fn mac_string(mac: [u8; 6]) -> String {
    mac.iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(":")
}

/// The hardware MAC of the adapter holding `bind_ip`.
///
/// Deep Symmetry's virtual-CDJ notes say to announce "your actual MAC and
/// IP addresses"; the 2026-08-19 hardware session showed why. Announcing a
/// synthetic MAC with the correct IP got decktective counted as a peer by
/// two CDJ-3000s and a DJM-900NXS2 — their keep-alives' peer counts went up
/// — while not one status packet ever arrived, which is what it looks like
/// when the players address their status frames straight to the MAC from
/// the keep-alive instead of ARPing for the IP.
///
/// `None` on Windows (needs `GetAdaptersInfo`, not `getifaddrs`) and
/// whenever the adapter can't be found, leaving the caller to fall back to
/// [`presence_mac`].
#[cfg(unix)]
fn interface_mac(bind_ip: Ipv4Addr) -> Option<[u8; 6]> {
    // Two passes over one `getifaddrs` list: find the interface name that
    // holds `bind_ip` (an AF_INET entry), then that same name's link-layer
    // entry, which is where the MAC lives.
    let name = interface_entries(|entries| {
        entries
            .iter()
            .find(|(_, address)| matches!(address, InterfaceAddress::V4(ip) if *ip == bind_ip))
            .map(|(name, _)| name.clone())
    })?;
    interface_entries(|entries| {
        entries
            .iter()
            .find_map(|(entry_name, address)| match address {
                InterfaceAddress::Link(mac) if *entry_name == name && *mac != [0; 6] => Some(*mac),
                _ => None,
            })
    })
}

#[cfg(not(unix))]
fn interface_mac(_bind_ip: Ipv4Addr) -> Option<[u8; 6]> {
    None
}

#[cfg(unix)]
enum InterfaceAddress {
    V4(Ipv4Addr),
    Link([u8; 6]),
}

/// Walks `getifaddrs` once and hands the collected `(name, address)` pairs
/// to `read`, freeing the list before returning whatever `read` extracted.
///
/// The unsafe block is confined here: everything above works on plain owned
/// values, and the list is freed on every path out.
#[cfg(unix)]
fn interface_entries<T>(
    read: impl FnOnce(&[(String, InterfaceAddress)]) -> Option<T>,
) -> Option<T> {
    let mut list: *mut libc::ifaddrs = std::ptr::null_mut();
    // SAFETY: `getifaddrs` either returns 0 and fills `list` with a
    // null-terminated linked list it owns, or returns non-zero and leaves
    // it untouched; we check the return before dereferencing.
    if unsafe { libc::getifaddrs(&mut list) } != 0 {
        return None;
    }

    let mut entries: Vec<(String, InterfaceAddress)> = Vec::new();
    let mut current = list;
    while !current.is_null() {
        // SAFETY: `current` is non-null and, until the list is freed below,
        // points at an `ifaddrs` whose `ifa_name` is a valid C string and
        // whose `ifa_addr` is either null or a `sockaddr` whose `sa_family`
        // says which concrete type it is.
        unsafe {
            let entry = &*current;
            current = entry.ifa_next;
            if entry.ifa_addr.is_null() {
                continue;
            }
            let name = std::ffi::CStr::from_ptr(entry.ifa_name)
                .to_string_lossy()
                .into_owned();
            match i32::from((*entry.ifa_addr).sa_family) {
                libc::AF_INET => {
                    let address = &*(entry.ifa_addr as *const libc::sockaddr_in);
                    entries.push((
                        name,
                        InterfaceAddress::V4(Ipv4Addr::from(u32::from_be(address.sin_addr.s_addr))),
                    ));
                }
                #[cfg(any(
                    target_os = "macos",
                    target_os = "ios",
                    target_os = "freebsd",
                    target_os = "netbsd",
                    target_os = "openbsd"
                ))]
                libc::AF_LINK => {
                    let address = &*(entry.ifa_addr as *const libc::sockaddr_dl);
                    let start = usize::from(address.sdl_nlen);
                    if usize::from(address.sdl_alen) == 6 && start + 6 <= address.sdl_data.len() {
                        let mut mac = [0u8; 6];
                        for (index, byte) in mac.iter_mut().enumerate() {
                            *byte = address.sdl_data[start + index] as u8;
                        }
                        entries.push((name, InterfaceAddress::Link(mac)));
                    }
                }
                #[cfg(target_os = "linux")]
                libc::AF_PACKET => {
                    let address = &*(entry.ifa_addr as *const libc::sockaddr_ll);
                    if address.sll_halen == 6 {
                        let mut mac = [0u8; 6];
                        mac.copy_from_slice(&address.sll_addr[..6]);
                        entries.push((name, InterfaceAddress::Link(mac)));
                    }
                }
                _ => {}
            }
        }
    }

    let extracted = read(&entries);
    // SAFETY: `list` came from a successful `getifaddrs` and nothing above
    // freed it; `entries` owns copies, not borrows, of its contents.
    unsafe { libc::freeifaddrs(list) };
    extracted
}

fn pick_device_number(taken: &[u8], preferred_override: Option<u8>) -> Option<u8> {
    let valid_override = preferred_override.filter(|number| (1..=6).contains(number));
    valid_override
        .into_iter()
        .chain(CANDIDATE_DEVICE_NUMBERS)
        .find(|number| !taken.contains(number))
}

/// Whether `device` is decktective's own presence broadcast rather than a
/// real or simulated peer — recognized by the MAC we are announcing with,
/// so a genuine collision (another device on our number, with its own MAC)
/// is still detected as a conflict. Public so
/// callers building on [`DeviceWatcher`]'s list (e.g. mapping it to deck
/// connect/disconnect events) can filter our own entry out the same way.
pub fn is_own_broadcast(device: &DiscoveredDevice) -> bool {
    match ANNOUNCED_MAC.get() {
        Some(&mac) => device.mac_address == mac_string(mac),
        // Not announcing (yet): fall back to recognizing the synthetic MAC
        // shape, so callers behave as they did before presence started.
        None => device.mac_address == mac_string(presence_mac(device.device_number)),
    }
}

/// Owns decktective's claimed virtual player number, once one is picked.
pub struct Presence {
    device_number_tx: watch::Sender<Option<u8>>,
    /// Kept alive purely so `device_number_tx` always has at least one
    /// receiver: `watch::Sender::send` fails once every receiver is gone,
    /// which — with no receiver ever kept — would otherwise silently drop
    /// every claimed number and leave [`Self::device_number`] stuck at
    /// `None` forever, receivers or not.
    device_number_rx: watch::Receiver<Option<u8>>,
    /// A specific number to prefer over [`PREFERRED_DEVICE_NUMBER`], e.g.
    /// from decktective's `DECKTECTIVE_VCDJ_ID` escape hatch (see the
    /// project's README) for claiming a number in the 1-4 range metadata
    /// queries need once a real device frees one up. Out-of-range values
    /// (outside 1-6) are ignored rather than rejected here — the caller
    /// owns validating user input; this just quietly falls back to the
    /// normal default instead of ever transmitting garbage.
    preferred_override: Option<u8>,
}

impl Presence {
    pub fn new(preferred_override: Option<u8>) -> Arc<Self> {
        let (device_number_tx, device_number_rx) = watch::channel(None);
        Arc::new(Self {
            device_number_tx,
            device_number_rx,
            preferred_override,
        })
    }

    pub fn subscribe(&self) -> watch::Receiver<Option<u8>> {
        self.device_number_tx.subscribe()
    }

    pub fn device_number(&self) -> Option<u8> {
        *self.device_number_rx.borrow()
    }

    /// Runs until the process exits: claims a number, broadcasts a
    /// settled-state keep-alive for it on every tick, and re-claims if
    /// another device is later seen using our number. `name` is what shows
    /// up as decktective's device name in other tools' device lists.
    ///
    /// `bind_ip` is the specific network adapter to send from — unlike a
    /// receiver (fine to bind `UNSPECIFIED` and take broadcasts arriving on
    /// any interface, see [`crate::discovery::DeviceWatcher`]), a broadcast
    /// *send* on a multi-NIC machine needs a concrete source interface or
    /// the OS routing table picks one, which is not reliably the booth's
    /// adapter (mirrors `decktective_timecode`'s `bind_socket`).
    ///
    /// A bind failure is logged and this task simply never broadcasts,
    /// rather than taking the engine down over one socket — status/beat
    /// listening keeps working regardless, since those don't depend on
    /// presence (see this module's doc comment).
    pub async fn run(
        self: Arc<Self>,
        device_watcher: Arc<DeviceWatcher>,
        name: String,
        bind_ip: Ipv4Addr,
    ) {
        // Real players broadcast their keep-alives *from* ANNOUNCE_PORT, so
        // that is what we do; an ephemeral source port is the fallback
        // rather than the default, since a peer that filters on the source
        // port would silently never send us status. Sharing the port with
        // this process's own listener is what `bind_udp_socket_on`'s
        // SO_REUSEADDR/SO_REUSEPORT is for.
        let socket = match crate::bind_udp_socket_on(bind_ip, ANNOUNCE_PORT) {
            Ok(socket) => socket,
            Err(error) => {
                tracing::warn!(%error, port = ANNOUNCE_PORT, "could not send presence from the standard announce port; falling back to an ephemeral one");
                match UdpSocket::bind((bind_ip, 0)).await.and_then(|socket| {
                    socket.set_broadcast(true)?;
                    Ok(socket)
                }) {
                    Ok(socket) => socket,
                    Err(error) => {
                        tracing::error!(%error, "failed to open the Pro DJ Link presence socket");
                        return;
                    }
                }
            }
        };

        // Resolved once, before the first keep-alive goes out: this is the
        // identity every player on the network will file us under, and
        // changing it mid-session would leave stale peer entries behind.
        let mac = match interface_mac(bind_ip) {
            Some(mac) => mac,
            None => {
                tracing::warn!(%bind_ip, "could not read the adapter's hardware MAC; announcing a synthetic one, which real players may not send status packets back to");
                presence_mac(PREFERRED_DEVICE_NUMBER)
            }
        };
        let _ = ANNOUNCED_MAC.set(mac);
        let mac = *ANNOUNCED_MAC.get().unwrap_or(&mac);
        tracing::info!(mac = %mac_string(mac), %bind_ip, "announcing decktective's presence");

        let mut devices_rx = device_watcher.subscribe();
        self.ensure_claimed(&device_watcher.devices());

        let mut tick = tokio::time::interval(KEEPALIVE_INTERVAL);
        loop {
            tokio::select! {
                _ = tick.tick() => {
                    let devices = device_watcher.devices();
                    self.ensure_claimed(&devices);
                    if let Some(number) = self.device_number() {
                        let peer_count = devices.iter().filter(|d| !is_own_broadcast(d)).count() as u8 + 1;
                        let packet = build_keepalive(number, &name, mac, bind_ip, peer_count);
                        if let Err(error) = socket.send_to(&packet, (BROADCAST_ADDR, ANNOUNCE_PORT)).await {
                            tracing::warn!(%error, "failed to broadcast our presence keep-alive");
                        }
                    }
                }
                changed = devices_rx.changed() => {
                    if changed.is_err() {
                        break; // The device watcher was dropped.
                    }
                    self.ensure_claimed(&devices_rx.borrow_and_update());
                }
            }
        }
    }

    /// Picks a number if we don't have one yet, or re-picks if the one we
    /// hold has been taken over by a real device in the meantime — our own
    /// broadcast is excluded from that check via [`is_own_broadcast`], or every tick
    /// would see "our" number as freshly conflicting with itself.
    fn ensure_claimed(&self, devices: &[DiscoveredDevice]) {
        let current = self.device_number();
        let taken: Vec<u8> = devices
            .iter()
            .filter(|device| !is_own_broadcast(device))
            .map(|device| device.device_number)
            .collect();

        if current.is_some_and(|number| !taken.contains(&number)) {
            return; // Still ours.
        }

        match pick_device_number(&taken, self.preferred_override) {
            Some(number) => {
                if current != Some(number) {
                    tracing::info!(number, "claimed Pro DJ Link virtual player number");
                }
                let _ = self.device_number_tx.send(Some(number));
            }
            None => {
                tracing::warn!(
                    "no free Pro DJ Link player number (1-6); presence disabled until one frees up"
                );
                let _ = self.device_number_tx.send(None);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::DeviceKind;

    fn device(number: u8, mac: &str) -> DiscoveredDevice {
        DiscoveredDevice {
            name: "CDJ-3000".to_string(),
            device_number: number,
            kind: DeviceKind::Player,
            ip: "192.168.1.10".to_string(),
            mac_address: mac.to_string(),
            peer_count: 1,
        }
    }

    #[test]
    fn prefers_the_default_number_when_free() {
        assert_eq!(pick_device_number(&[], None), Some(PREFERRED_DEVICE_NUMBER));
    }

    #[test]
    fn falls_back_to_six_then_one_through_four() {
        assert_eq!(pick_device_number(&[5], None), Some(6));
        assert_eq!(pick_device_number(&[5, 6], None), Some(1));
        assert_eq!(pick_device_number(&[5, 6, 1, 2, 3], None), Some(4));
    }

    #[test]
    fn none_free_when_the_whole_range_is_taken() {
        assert_eq!(pick_device_number(&[1, 2, 3, 4, 5, 6], None), None);
    }

    #[test]
    fn an_override_is_tried_before_the_default() {
        assert_eq!(pick_device_number(&[], Some(2)), Some(2));
    }

    #[test]
    fn a_taken_override_falls_back_to_the_normal_order() {
        assert_eq!(
            pick_device_number(&[2], Some(2)),
            Some(PREFERRED_DEVICE_NUMBER)
        );
    }

    #[test]
    fn an_out_of_range_override_is_ignored() {
        assert_eq!(
            pick_device_number(&[], Some(33)),
            Some(PREFERRED_DEVICE_NUMBER)
        );
        assert_eq!(
            pick_device_number(&[], Some(0)),
            Some(PREFERRED_DEVICE_NUMBER)
        );
    }

    #[test]
    fn our_own_broadcast_does_not_look_like_a_conflict() {
        let presence = Presence::new(None);
        let devices = vec![device(5, &mac_string(presence_mac(5)))];
        presence.ensure_claimed(&devices);
        assert_eq!(presence.device_number(), Some(5));

        // Seeing our own keep-alive echoed back must not trigger a re-pick.
        presence.ensure_claimed(&devices);
        assert_eq!(presence.device_number(), Some(5));
    }

    #[test]
    fn a_real_device_taking_our_number_triggers_a_reclaim() {
        let presence = Presence::new(None);
        presence.ensure_claimed(&[device(5, &mac_string(presence_mac(5)))]);
        assert_eq!(presence.device_number(), Some(5));

        // A different MAC on our number is a genuine conflict, not us.
        presence.ensure_claimed(&[device(5, "aa:bb:cc:dd:ee:ff")]);
        assert_eq!(presence.device_number(), Some(6));
    }
}
