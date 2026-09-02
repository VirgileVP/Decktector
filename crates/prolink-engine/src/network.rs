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

use decktective_net::{list_broadcastable_interfaces, resolve_ipv4, NetworkInterfaceInfo};

/// The adapter to bind decktective's own broadcast socket to, given the IP
/// of a peer already seen on the network, if any yet.
pub fn pick_interface(known_peer: Option<Ipv4Addr>) -> anyhow::Result<Ipv4Addr> {
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
}
