//! Claims a virtual-CDJ identity for Decktector on the Pro DJ Link network.
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
//! tests. This is enough for decktector to show up as a peer in its own
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
use std::sync::{Arc, RwLock};
use std::time::Duration;

use tokio::net::UdpSocket;
use tokio::sync::watch;

use crate::discovery::{build_keepalive, DeviceWatcher, DiscoveredDevice};
use crate::ANNOUNCE_PORT;

/// Matches real players' own keep-alive cadence (see
/// [`crate::discovery`]'s module doc).
const KEEPALIVE_INTERVAL: Duration = Duration::from_millis(1500);

/// Decktector's default virtual player number when available — chosen as 7
/// (the standard virtual CDJ identifier, used by Rekordbox Link Export and
/// ShowKontrol) so it never collides with physical CDJ hardware (which only
/// supports player numbers 1 to 6).
const PREFERRED_DEVICE_NUMBER: u8 = 7;

/// Candidate order for virtual player identity: prefer 7 first (never assigned
/// to physical CDJs), then fall back to 6 and 5 (only used on expanded 6-deck setups),
/// and finally 4, 3, 2, 1 if all higher slots are occupied.
const CANDIDATE_DEVICE_NUMBERS: [u8; 7] = [PREFERRED_DEVICE_NUMBER, 6, 5, 4, 3, 2, 1];

const BROADCAST_ADDR: Ipv4Addr = Ipv4Addr::new(255, 255, 255, 255);

/// The MAC our keep-alives actually went out with, once [`Presence::run`]
/// has resolved it. Process-global because it is a process-global fact —
/// there is one Pro DJ Link identity per process — and because
/// [`is_own_broadcast`] is called from code that has no handle on
/// [`Presence`] (`prolink_engine::engine`'s deck mapping). Updated if
/// decktector re-binds to a different network interface.
static ANNOUNCED_MAC: RwLock<Option<[u8; 6]>> = RwLock::new(None);

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
/// synthetic MAC with the correct IP got decktector counted as a peer by
/// two CDJ-3000s and a DJM-900NXS2 — their keep-alives' peer counts went up
/// — while not one status packet ever arrived, which is what it looks like
/// when the players address their status frames straight to the MAC from
/// the keep-alive instead of ARPing for the IP.
///
/// `None` whenever the adapter can't be found, leaving the caller to fall
/// back to [`presence_mac`]. Implemented per platform because there is no
/// portable way to read a MAC: `getifaddrs` here, `GetAdaptersAddresses`
/// on Windows (below).
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

/// The Windows counterpart of the `getifaddrs` walk above: one
/// `GetAdaptersAddresses` call, then a scan of each adapter's unicast
/// addresses for `bind_ip`, whose adapter's `PhysicalAddress` is the MAC we
/// have to announce. Windows offers no `getifaddrs`, and `if_addrs` — which
/// the rest of decktector uses for addresses — does not expose the MAC.
///
/// Before this existed, Windows always fell through to [`presence_mac`],
/// which is exactly the synthetic-MAC case the 2026-08-19 hardware session
/// showed gets decktector counted as a peer and sent no status packets at
/// all: devices visible in the diagnostics panel (keep-alives are broadcast,
/// so they arrive regardless) and decks permanently empty on the main view.
#[cfg(windows)]
fn interface_mac(bind_ip: Ipv4Addr) -> Option<[u8; 6]> {
    use windows_sys::Win32::Foundation::{ERROR_BUFFER_OVERFLOW, ERROR_SUCCESS};
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        GetAdaptersAddresses, GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER,
        GAA_FLAG_SKIP_MULTICAST, IP_ADAPTER_ADDRESSES_LH,
    };
    use windows_sys::Win32::Networking::WinSock::{AF_INET, SOCKADDR_IN};

    /// Only the unicast addresses and the physical address are read, so the
    /// rest of what the call can gather is skipped.
    const FLAGS: u32 = GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER;
    /// Microsoft's own sample starts at 15 KB to avoid the near-certain
    /// second call a smaller guess costs; a booth machine's adapter list
    /// fits well inside it.
    const INITIAL_BYTES: u32 = 16 * 1024;
    /// The needed size can grow between calls (an adapter appearing), so the
    /// retry is bounded rather than a `loop`.
    const ATTEMPTS: usize = 3;
    /// Length of an Ethernet MAC; `PhysicalAddress` is a fixed 8-byte field
    /// with only `PhysicalAddressLength` of it meaningful.
    const MAC_LEN: usize = 6;

    let mut size = INITIAL_BYTES;
    for _ in 0..ATTEMPTS {
        // A `Vec<u64>` rather than `Vec<u8>`: the call writes a linked list
        // of `IP_ADAPTER_ADDRESSES_LH` into this buffer, which a byte
        // allocation is not guaranteed to be aligned for.
        let mut buffer = vec![0u64; (size as usize).div_ceil(size_of::<u64>())];
        let adapters = buffer.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();

        // SAFETY: `adapters` points at `size` writable bytes aligned for the
        // struct being written, and `size` is a live `u32` the call may
        // overwrite with the size it actually needs.
        let result = unsafe {
            GetAdaptersAddresses(
                u32::from(AF_INET),
                FLAGS,
                std::ptr::null(),
                adapters,
                &mut size,
            )
        };
        match result {
            // `size` now holds what the call needs; go round with a bigger
            // buffer.
            ERROR_BUFFER_OVERFLOW => continue,
            ERROR_SUCCESS => {}
            _ => return None,
        }

        // SAFETY: the call succeeded, so `adapters` heads a null-terminated
        // list living in `buffer` (not freed until this iteration ends),
        // each entry's `Next`/`FirstUnicastAddress` either null or pointing
        // into that same buffer, and each non-null `lpSockaddr` pointing at
        // a `sockaddr` whose `sa_family` says which concrete type it is.
        let mac = unsafe {
            let mut found = None;
            let mut adapter = adapters.cast_const();
            'adapters: while !adapter.is_null() {
                let entry = &*adapter;
                adapter = entry.Next.cast_const();
                if entry.PhysicalAddressLength as usize != MAC_LEN {
                    continue; // Loopback, tunnels, anything without an Ethernet MAC.
                }
                let mut unicast = entry.FirstUnicastAddress.cast_const();
                while !unicast.is_null() {
                    let address = &*unicast;
                    unicast = address.Next.cast_const();
                    let sockaddr = address.Address.lpSockaddr;
                    if sockaddr.is_null() || (*sockaddr).sa_family != AF_INET {
                        continue;
                    }
                    let v4 = &*sockaddr.cast::<SOCKADDR_IN>();
                    if Ipv4Addr::from(u32::from_be(v4.sin_addr.S_un.S_addr)) == bind_ip {
                        let mut mac = [0u8; MAC_LEN];
                        mac.copy_from_slice(&entry.PhysicalAddress[..MAC_LEN]);
                        found = Some(mac);
                        break 'adapters;
                    }
                }
            }
            found
        };
        return mac;
    }
    None
}

#[cfg(not(any(unix, windows)))]
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
    let valid_override = preferred_override.filter(|number| (1..=7).contains(number));
    valid_override
        .into_iter()
        .chain(CANDIDATE_DEVICE_NUMBERS)
        .find(|number| !taken.contains(number))
}

/// Whether `device` is decktector's own presence broadcast rather than a
/// real or simulated peer — recognized by the MAC we are announcing with,
/// so a genuine collision (another device on our number, with its own MAC)
/// is still detected as a conflict. Public so
/// callers building on [`DeviceWatcher`]'s list (e.g. mapping it to deck
/// connect/disconnect events) can filter our own entry out the same way.
pub fn is_own_broadcast(device: &DiscoveredDevice) -> bool {
    if device.name.eq_ignore_ascii_case("decktector") {
        return true;
    }
    let guard = ANNOUNCED_MAC.read().unwrap_or_else(|p| p.into_inner());
    match *guard {
        Some(mac) => device.mac_address == mac_string(mac),
        // Not announcing (yet): fall back to recognizing the synthetic MAC
        // shape, so callers behave as they did before presence started.
        None => device.mac_address == mac_string(presence_mac(device.device_number)),
    }
}

/// The peer count our keep-alive reports: every other device on the network
/// plus ourselves, matching how real players count (see
/// [`DiscoveredDevice::peer_count`]). Counted per physical device, so two
/// mixers announcing the same number 33 count as two peers.
fn peer_count(devices: &[DiscoveredDevice]) -> u8 {
    let others = devices
        .iter()
        .filter(|device| !is_own_broadcast(device))
        .count();
    u8::try_from(others + 1).unwrap_or(u8::MAX)
}

/// Owns decktector's claimed virtual player number, once one is picked.
pub struct Presence {
    device_number_tx: watch::Sender<Option<u8>>,
    /// Kept alive purely so `device_number_tx` always has at least one
    /// receiver: `watch::Sender::send` fails once every receiver is gone,
    /// which — with no receiver ever kept — would otherwise silently drop
    /// every claimed number and leave [`Self::device_number`] stuck at
    /// `None` forever, receivers or not.
    device_number_rx: watch::Receiver<Option<u8>>,
    /// A specific number to prefer over [`PREFERRED_DEVICE_NUMBER`], e.g.
    /// from decktector's `DECKTECTOR_VCDJ_ID` escape hatch (see the
    /// project's README) for claiming a number in the 1-4 range metadata
    /// queries need once a real device frees one up. Out-of-range values
    /// (outside 1-7) are ignored rather than rejected here — the caller
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
    /// up as decktector's device name in other tools' device lists.
    ///
    /// `bind_ip` is the specific network adapter to send from — unlike a
    /// receiver (fine to bind `UNSPECIFIED` and take broadcasts arriving on
    /// any interface, see [`crate::discovery::DeviceWatcher`]), a broadcast
    /// *send* on a multi-NIC machine needs a concrete source interface or
    /// the OS routing table picks one, which is not reliably the booth's
    /// adapter (mirrors `decktector_timecode`'s `bind_socket`).
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
        let mut socket = match open_socket(bind_ip).await {
            Ok(socket) => Some(socket),
            Err(error) => {
                tracing::error!(%error, "failed to open the Pro DJ Link presence socket");
                return;
            }
        };

        // Resolved before the first keep-alive goes out for this interface:
        // this is the identity players on this network will file us under.
        let mac = match interface_mac(bind_ip) {
            Some(mac) => mac,
            None => {
                tracing::warn!(%bind_ip, "could not read the adapter's hardware MAC; announcing a synthetic one, which real players may not send status packets back to");
                presence_mac(PREFERRED_DEVICE_NUMBER)
            }
        };
        *ANNOUNCED_MAC.write().unwrap_or_else(|p| p.into_inner()) = Some(mac);
        tracing::info!(mac = %mac_string(mac), %bind_ip, "announcing decktector's presence");

        let mut devices_rx = device_watcher.subscribe();
        self.ensure_claimed(&device_watcher.devices());

        let mut tick = tokio::time::interval(KEEPALIVE_INTERVAL);
        loop {
            tokio::select! {
                _ = tick.tick() => {
                    let devices = device_watcher.devices();
                    self.ensure_claimed(&devices);
                    if let Some(number) = self.device_number() {
                        if socket.is_none() {
                            socket = reopen_socket(bind_ip).await;
                        }
                        if let Some(open) = &socket {
                            let packet = build_keepalive(number, &name, mac, bind_ip, peer_count(&devices));
                            if let Err(error) = open.send_to(&packet, (BROADCAST_ADDR, ANNOUNCE_PORT)).await {
                                // A socket bound to an adapter's address can
                                // stay dead after the link drops, even once
                                // the same address is back; a fresh one is
                                // what gets keep-alives — and so status —
                                // flowing again.
                                tracing::warn!(%error, "failed to broadcast our presence keep-alive; reopening the socket");
                                socket = None;
                            }
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
}

/// Opens the socket keep-alives leave from, on `bind_ip`.
///
/// Real players broadcast their keep-alives *from* [`ANNOUNCE_PORT`], so
/// that is what we do; an ephemeral source port is the fallback rather than
/// the default, since a peer that filters on the source port would silently
/// never send us status. Sharing the port with this process's own listener
/// is what `bind_udp_socket_on`'s SO_REUSEADDR/SO_REUSEPORT is for.
async fn open_socket(bind_ip: Ipv4Addr) -> std::io::Result<UdpSocket> {
    let announce_port_error = match crate::bind_udp_socket_on(bind_ip, ANNOUNCE_PORT) {
        Ok(socket) => return Ok(socket),
        Err(error) => error,
    };
    let socket = UdpSocket::bind((bind_ip, 0)).await?;
    socket.set_broadcast(true)?;
    tracing::warn!(error = %announce_port_error, port = ANNOUNCE_PORT, "could not send presence from the standard announce port; fell back to an ephemeral one");
    Ok(socket)
}

/// [`open_socket`] again after a failed send. Failing here is expected while
/// the link is still down, so it is retried on the next tick rather than
/// reported each time.
async fn reopen_socket(bind_ip: Ipv4Addr) -> Option<UdpSocket> {
    match open_socket(bind_ip).await {
        Ok(socket) => {
            tracing::info!(%bind_ip, "reopened the Pro DJ Link presence socket");
            Some(socket)
        }
        Err(error) => {
            tracing::debug!(%error, %bind_ip, "presence socket still cannot be reopened");
            None
        }
    }
}

impl Presence {
    /// Picks a number if we don't have one yet, or re-picks if the one we
    /// hold has been taken over by a real device in the meantime — our own
    /// broadcast is excluded from that check via [`is_own_broadcast`], or every tick
    /// would see "our" number as freshly conflicting with itself. Devices
    /// sharing a number among themselves (two mixers on 33) just contribute
    /// that number to the taken set once, as far as this check goes.
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
                    "no free Pro DJ Link player number (1-7); presence disabled until one frees up"
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
            number_holders: 1,
        }
    }

    #[test]
    fn prefers_the_default_number_when_free() {
        assert_eq!(pick_device_number(&[], None), Some(7));
        assert_eq!(pick_device_number(&[1, 2, 3, 4], None), Some(7));
    }

    #[test]
    fn falls_back_through_seven_six_five_then_four_three_two_one() {
        assert_eq!(pick_device_number(&[7], None), Some(6));
        assert_eq!(pick_device_number(&[7, 6], None), Some(5));
        assert_eq!(pick_device_number(&[7, 6, 5], None), Some(4));
        assert_eq!(pick_device_number(&[7, 6, 5, 4], None), Some(3));
        assert_eq!(pick_device_number(&[7, 6, 5, 4, 3], None), Some(2));
        assert_eq!(pick_device_number(&[7, 6, 5, 4, 3, 2], None), Some(1));
    }

    #[test]
    fn none_free_when_the_whole_range_is_taken() {
        assert_eq!(pick_device_number(&[1, 2, 3, 4, 5, 6, 7], None), None);
    }

    #[test]
    fn an_override_is_tried_before_the_default() {
        assert_eq!(pick_device_number(&[], Some(2)), Some(2));
        assert_eq!(pick_device_number(&[], Some(5)), Some(5));
    }

    #[test]
    fn a_taken_override_falls_back_to_the_normal_order() {
        assert_eq!(pick_device_number(&[2], Some(2)), Some(7));
    }

    #[test]
    fn an_out_of_range_override_is_ignored() {
        assert_eq!(pick_device_number(&[], Some(33)), Some(7));
        assert_eq!(pick_device_number(&[], Some(0)), Some(7));
    }

    #[test]
    fn our_own_broadcast_does_not_look_like_a_conflict() {
        let presence = Presence::new(None);
        let devices = vec![device(7, &mac_string(presence_mac(7)))];
        presence.ensure_claimed(&devices);
        assert_eq!(presence.device_number(), Some(7));

        // Seeing our own keep-alive echoed back must not trigger a re-pick.
        presence.ensure_claimed(&devices);
        assert_eq!(presence.device_number(), Some(7));
    }

    #[test]
    fn our_own_broadcast_recognized_by_name() {
        let mut d = device(7, "aa:bb:cc:dd:ee:ff");
        d.name = "decktector".to_string();
        assert!(is_own_broadcast(&d));
    }

    #[test]
    fn a_real_device_taking_our_number_triggers_a_reclaim() {
        let presence = Presence::new(None);
        presence.ensure_claimed(&[device(7, &mac_string(presence_mac(7)))]);
        assert_eq!(presence.device_number(), Some(7));

        // A different MAC on our number is a genuine conflict, not us.
        presence.ensure_claimed(&[device(7, "aa:bb:cc:dd:ee:ff")]);
        assert_eq!(presence.device_number(), Some(6));
    }

    #[test]
    fn peer_count_counts_each_device_sharing_a_number() {
        let mut first_mixer = device(33, "00:11:22:33:44:a9");
        first_mixer.kind = DeviceKind::Mixer;
        let mut second_mixer = device(33, "00:11:22:33:44:b0");
        second_mixer.kind = DeviceKind::Mixer;
        let player = device(1, "aa:bb:cc:dd:ee:01");

        // Two mixers and a player, plus ourselves.
        assert_eq!(peer_count(&[player, first_mixer, second_mixer]), 4);
    }

    #[test]
    fn peer_count_excludes_our_own_broadcast() {
        let own = device(7, &mac_string(presence_mac(7)));
        assert_eq!(peer_count(&[own]), 1);
    }

    #[test]
    fn a_number_shared_between_two_other_devices_does_not_disturb_our_claim() {
        let presence = Presence::new(None);
        let devices = [
            device(33, "00:11:22:33:44:a9"),
            device(33, "00:11:22:33:44:b0"),
        ];
        presence.ensure_claimed(&devices);
        assert_eq!(presence.device_number(), Some(7));
    }

    /// An address from TEST-NET-1 (RFC 5737), which no adapter holds.
    const UNASSIGNED_IP: Ipv4Addr = Ipv4Addr::new(192, 0, 2, 1);

    #[test]
    fn an_address_no_adapter_holds_has_no_mac() {
        assert_eq!(interface_mac(UNASSIGNED_IP), None);
    }

    /// Guards against a platform silently falling back to the synthetic MAC,
    /// as Windows did before it had its own `interface_mac`. Asserts on *any*
    /// adapter rather than the first one, since a tunnel adapter (VPN, PPP)
    /// legitimately has no Ethernet MAC; a host with no IPv4 adapter besides
    /// loopback has nothing to check.
    #[test]
    fn some_host_adapter_resolves_to_its_hardware_mac() {
        let host_ips: Vec<Ipv4Addr> = if_addrs::get_if_addrs()
            .expect("list the host's network interfaces")
            .into_iter()
            .filter(|interface| !interface.is_loopback())
            .filter_map(|interface| match interface.ip() {
                std::net::IpAddr::V4(ip) => Some(ip),
                std::net::IpAddr::V6(_) => None,
            })
            .collect();
        if host_ips.is_empty() {
            return;
        }
        assert!(
            host_ips
                .iter()
                .any(|ip| interface_mac(*ip).is_some_and(|mac| mac != [0; 6])),
            "no hardware MAC found for any of {host_ips:?}"
        );
    }
}
