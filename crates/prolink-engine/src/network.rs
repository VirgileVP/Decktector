//! Picks which local network adapter to broadcast decktective's own
//! presence — and originate dbserver metadata connections — from, on a
//! machine that might have more than one (see `decktective_net`'s own
//! module doc — a booth machine routinely does: Ethernet to the players,
//! Wi-Fi for everything else).
//!
//! Uses the one adapter when there's only one, otherwise matches a
//! discovered peer's IP against each adapter's real netmask — the same
//! exact-match approach decktective's pre-Rust engine used
//! (`findInterfaceForPeer` in `network-config.ts`, `(candidateIp & mask) ==
//! (peerIp & mask)`), now that `decktective_net::NetworkInterfaceInfo`
//! carries a real netmask instead of the earlier phase's /24-or-/16 guess.

use std::net::Ipv4Addr;
use std::sync::Arc;

use decktective_net::{list_broadcastable_interfaces, resolve_ipv4, NetworkInterfaceInfo};
use tokio::sync::watch;

/// The adapter the user pinned Pro DJ Link to in settings, if any.
///
/// The subnet auto-pick below cannot tell apart several adapters sharing a
/// subnet — on Windows every unconfigured adapter (Bluetooth PAN, Wi-Fi
/// Direct, the booth Ethernet) self-assigns a 169.254.0.0/16 link-local
/// address, so a CDJ on that range matches whichever comes first. Pinning
/// the adapter by name settles it. `None` keeps the auto-pick.
///
/// A watch channel rather than a plain lock so the engine's reporting loop
/// wakes on a change and rebinds straight away, not on the next device-list
/// change.
pub struct InterfacePreference(watch::Sender<Option<String>>);

impl InterfacePreference {
    pub fn new() -> Arc<Self> {
        Arc::new(Self(watch::channel(None).0))
    }

    /// Pins presence to the named adapter; `None` goes back to the auto-pick.
    pub fn set(&self, interface_name: Option<String>) {
        self.0.send_if_modified(|current| {
            if *current == interface_name {
                return false;
            }
            *current = interface_name;
            true
        });
    }

    pub fn subscribe(&self) -> watch::Receiver<Option<String>> {
        self.0.subscribe()
    }
}

/// The adapter to bind decktective's own broadcast socket to, given the IP
/// of a peer already seen on the network, if any yet, and the adapter the
/// user pinned, if any. A pinned adapter that has gone away (unplugged, no
/// IPv4 address) falls back to the auto-pick rather than leaving the
/// engine unbound.
pub fn pick_interface(
    known_peer: Option<Ipv4Addr>,
    preferred: Option<&str>,
) -> anyhow::Result<Ipv4Addr> {
    if let Some(name) = preferred {
        match resolve_ipv4(name) {
            Ok(ip) => return Ok(ip),
            Err(error) => tracing::warn!(
                %error,
                interface = name,
                "chosen Pro DJ Link interface unavailable; falling back to auto-pick"
            ),
        }
    }
    let interfaces = list_broadcastable_interfaces()?;
    let interface = match interfaces.as_slice() {
        [] => anyhow::bail!("no broadcastable network interface found"),
        [only] => only,
        many => known_peer
            .and_then(|peer| many.iter().find(|candidate| shares_subnet(candidate, peer)))
            .unwrap_or(&many[0]),
    };
    resolve_ipv4(&interface.name)
}

fn shares_subnet(interface: &NetworkInterfaceInfo, peer: Ipv4Addr) -> bool {
    let (Ok(ip), Ok(mask)) = (
        interface.ip.parse::<Ipv4Addr>(),
        interface.netmask.parse::<Ipv4Addr>(),
    ) else {
        return false;
    };
    (u32::from(ip) & u32::from(mask)) == (u32::from(peer) & u32::from(mask))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn interface(name: &str, ip: &str, netmask: &str) -> NetworkInterfaceInfo {
        NetworkInterfaceInfo {
            name: name.to_string(),
            ip: ip.to_string(),
            netmask: netmask.to_string(),
        }
    }

    #[test]
    fn matches_a_peer_on_the_same_slash_24() {
        let booth = interface("eth0", "192.168.1.20", "255.255.255.0");
        let wifi = interface("wlan0", "10.0.0.5", "255.255.255.0");
        let peer: Ipv4Addr = "192.168.1.10".parse().unwrap();

        assert!(shares_subnet(&booth, peer));
        assert!(!shares_subnet(&wifi, peer));
    }

    #[test]
    fn matches_a_link_local_peer_on_its_slash_16() {
        let booth = interface("eth0", "169.254.7.3", "255.255.0.0");
        let peer: Ipv4Addr = "169.254.99.1".parse().unwrap();

        assert!(shares_subnet(&booth, peer));
    }

    #[test]
    fn a_peer_outside_a_narrower_mask_does_not_match() {
        // Same first three octets, but a real /28 on the interface means
        // the peer (.99) is not actually on this link.
        let narrow = interface("eth0", "192.168.1.10", "255.255.255.240");
        let peer: Ipv4Addr = "192.168.1.99".parse().unwrap();

        assert!(!shares_subnet(&narrow, peer));
    }

    #[test]
    fn an_unparsable_interface_ip_never_matches() {
        let broken = interface("eth0", "not-an-ip", "255.255.255.0");
        let peer: Ipv4Addr = "192.168.1.10".parse().unwrap();

        assert!(!shares_subnet(&broken, peer));
    }

    #[test]
    fn an_unparsable_netmask_never_matches() {
        let broken = interface("eth0", "192.168.1.20", "not-a-mask");
        let peer: Ipv4Addr = "192.168.1.10".parse().unwrap();

        assert!(!shares_subnet(&broken, peer));
    }

    #[test]
    fn a_missing_preferred_interface_falls_back_to_the_auto_pick() {
        let auto = pick_interface(None, None).ok();
        let pinned_to_nothing = pick_interface(None, Some("no-such-adapter")).ok();

        assert_eq!(pinned_to_nothing, auto);
    }

    #[test]
    fn a_preference_change_is_seen_once_and_a_repeat_is_ignored() {
        let preference = InterfacePreference::new();
        let mut rx = preference.subscribe();

        preference.set(Some("eth0".to_string()));
        assert!(rx.has_changed().unwrap());
        assert_eq!(rx.borrow_and_update().as_deref(), Some("eth0"));

        preference.set(Some("eth0".to_string()));
        assert!(!rx.has_changed().unwrap());
    }
}
