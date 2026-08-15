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

use std::net::Ipv4Addr;
use std::sync::Arc;
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

/// Locally-administered MAC for our virtual identity, keyed by device
/// number — deterministic so re-deriving it after a renumber is trivial, and
/// so it's recognizable as "us" (not a real Pioneer OUI) in a packet capture.
fn presence_mac(device_number: u8) -> [u8; 6] {
    [0x02, 0x00, 0x00, 0x00, 0x00, device_number]
}

fn presence_mac_string(device_number: u8) -> String {
    presence_mac(device_number)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(":")
}

fn pick_device_number(taken: &[u8], preferred_override: Option<u8>) -> Option<u8> {
    let valid_override = preferred_override.filter(|number| (1..=6).contains(number));
    valid_override
        .into_iter()
        .chain(CANDIDATE_DEVICE_NUMBERS)
        .find(|number| !taken.contains(number))
}

/// Whether `device` is decktective's own presence broadcast rather than a
/// real or simulated peer — recognized by number *and* MAC matching what
/// [`presence_mac`] would generate for that number, so a genuine collision
/// (same number, different MAC) is still detected as a conflict. Public so
/// callers building on [`DeviceWatcher`]'s list (e.g. mapping it to deck
/// connect/disconnect events) can filter our own entry out the same way.
pub fn is_own_broadcast(device: &DiscoveredDevice) -> bool {
    device.mac_address == presence_mac_string(device.device_number)
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
    pub async fn run(self: Arc<Self>, device_watcher: Arc<DeviceWatcher>, name: String, bind_ip: Ipv4Addr) {
        let socket = match UdpSocket::bind((bind_ip, 0)).await {
            Ok(socket) => socket,
            Err(error) => {
                tracing::error!(%error, "failed to open the Pro DJ Link presence socket");
                return;
            }
        };
        if let Err(error) = socket.set_broadcast(true) {
            tracing::error!(%error, "failed to enable broadcast on the presence socket");
            return;
        }

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
                        let packet = build_keepalive(number, &name, presence_mac(number), peer_count);
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
        assert_eq!(pick_device_number(&[2], Some(2)), Some(PREFERRED_DEVICE_NUMBER));
    }

    #[test]
    fn an_out_of_range_override_is_ignored() {
        assert_eq!(pick_device_number(&[], Some(33)), Some(PREFERRED_DEVICE_NUMBER));
        assert_eq!(pick_device_number(&[], Some(0)), Some(PREFERRED_DEVICE_NUMBER));
    }

    #[test]
    fn our_own_broadcast_does_not_look_like_a_conflict() {
        let presence = Presence::new(None);
        let devices = vec![device(5, &presence_mac_string(5))];
        presence.ensure_claimed(&devices);
        assert_eq!(presence.device_number(), Some(5));

        // Seeing our own keep-alive echoed back must not trigger a re-pick.
        presence.ensure_claimed(&devices);
        assert_eq!(presence.device_number(), Some(5));
    }

    #[test]
    fn a_real_device_taking_our_number_triggers_a_reclaim() {
        let presence = Presence::new(None);
        presence.ensure_claimed(&[device(5, &presence_mac_string(5))]);
        assert_eq!(presence.device_number(), Some(5));

        // A different MAC on our number is a genuine conflict, not us.
        presence.ensure_claimed(&[device(5, "aa:bb:cc:dd:ee:ff")]);
        assert_eq!(presence.device_number(), Some(6));
    }
}
