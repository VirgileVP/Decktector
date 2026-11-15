//! Local network interface lookup.
//!
//! Both Art-Net timecode and the remote web view are broadcast or bound on
//! the local subnet, so the setting that matters for each is *which* network
//! adapter to use — a booth machine routinely has more than one (Ethernet to
//! the lighting network, Wi-Fi for everything else). A typed IP address
//! cannot express that choice reliably; the adapter can.

use std::net::Ipv4Addr;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One IPv4-capable network adapter, as offered to the settings panel.
///
/// `netmask` is carried alongside `ip` (rather than the two being combined
/// into a single CIDR string) so a consumer that only cares about the
/// address, like the settings panel, doesn't have to parse one back out —
/// [`prolink_engine`](../prolink_engine/index.html)'s interface auto-pick is
/// the one consumer that needs it, to match a discovered peer's subnet
/// exactly rather than guess at it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "NetworkInterfaceInfo.ts")]
pub struct NetworkInterfaceInfo {
    pub name: String,
    pub ip: String,
    pub netmask: String,
}

/// Every non-loopback interface this machine could broadcast or bind from.
pub fn list_broadcastable_interfaces() -> anyhow::Result<Vec<NetworkInterfaceInfo>> {
    let interfaces = if_addrs::get_if_addrs()?;
    Ok(interfaces
        .into_iter()
        .filter(|interface| !interface.is_loopback())
        .filter_map(|interface| match interface.addr {
            if_addrs::IfAddr::V4(v4) => Some(NetworkInterfaceInfo {
                name: interface.name,
                ip: v4.ip.to_string(),
                netmask: v4.netmask.to_string(),
            }),
            if_addrs::IfAddr::V6(_) => None,
        })
        .collect())
}

/// A subnet needs at least two host bits to have a broadcast address: a /31
/// is a point-to-point pair (RFC 3021) and a /32 a single host.
const MIN_HOST_BITS_FOR_BROADCAST: u32 = 2;

/// An adapter's IPv4 address and the subnet it sits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ipv4Subnet {
    pub ip: Ipv4Addr,
    pub netmask: Ipv4Addr,
}

impl Ipv4Subnet {
    /// The subnet's directed broadcast address (`ip | !netmask`, e.g.
    /// `2.255.255.255` for `2.0.0.10/8`): a broadcast sent there is routed
    /// out of this adapter, whereas `255.255.255.255` leaves through
    /// whichever adapter the OS picks — on macOS the one holding the
    /// default route, whatever address the socket is bound to. `None` for a
    /// subnet too small to have one (see [`MIN_HOST_BITS_FOR_BROADCAST`]).
    pub fn broadcast(self) -> Option<Ipv4Addr> {
        let host_bits = !u32::from(self.netmask);
        (host_bits.count_ones() >= MIN_HOST_BITS_FOR_BROADCAST)
            .then(|| Ipv4Addr::from(u32::from(self.ip) | host_bits))
    }
}

/// The IPv4 address of the named interface, to bind a socket or listener to.
///
/// Re-resolved on every config change rather than cached indefinitely: an
/// adapter can pick up a new address (DHCP) or disappear (unplugged) between
/// one set and the next.
pub fn resolve_ipv4(interface_name: &str) -> anyhow::Result<Ipv4Addr> {
    resolve_ipv4_subnet(interface_name).map(|subnet| subnet.ip)
}

/// Like [`resolve_ipv4`], with the netmask too — for a sender that needs
/// the subnet's broadcast address, not just an address to bind to.
pub fn resolve_ipv4_subnet(interface_name: &str) -> anyhow::Result<Ipv4Subnet> {
    // `get_if_addrs` yields one entry per (adapter, address) pair, so a dual
    // IPv4/IPv6 adapter shows up as two entries sharing the same name. A
    // plain `find` on the name alone can land on the IPv6 entry and miss the
    // IPv4 one further down the list — `find_map` keeps scanning past a
    // name match that isn't the address family we need.
    if_addrs::get_if_addrs()?
        .into_iter()
        .find_map(|interface| {
            if interface.name != interface_name {
                return None;
            }
            match interface.addr {
                if_addrs::IfAddr::V4(v4) => Some(Ipv4Subnet {
                    ip: v4.ip,
                    netmask: v4.netmask,
                }),
                if_addrs::IfAddr::V6(_) => None,
            }
        })
        .ok_or_else(|| anyhow::anyhow!("network interface '{interface_name}' has no IPv4 address"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn broadcast_of(ip: &str, netmask: &str) -> Option<Ipv4Addr> {
        Ipv4Subnet {
            ip: ip.parse().unwrap(),
            netmask: netmask.parse().unwrap(),
        }
        .broadcast()
    }

    #[test]
    fn a_lighting_network_broadcasts_on_its_own_subnet() {
        assert_eq!(
            broadcast_of("2.0.0.10", "255.0.0.0"),
            Some(Ipv4Addr::new(2, 255, 255, 255))
        );
        assert_eq!(
            broadcast_of("192.168.1.42", "255.255.255.0"),
            Some(Ipv4Addr::new(192, 168, 1, 255))
        );
        assert_eq!(
            broadcast_of("169.254.13.7", "255.255.0.0"),
            Some(Ipv4Addr::new(169, 254, 255, 255))
        );
    }

    #[test]
    fn a_subnet_too_small_for_a_broadcast_address_has_none() {
        assert_eq!(broadcast_of("10.0.0.1", "255.255.255.254"), None);
        assert_eq!(broadcast_of("10.0.0.1", "255.255.255.255"), None);
    }

    #[test]
    fn a_tiny_subnet_still_has_its_broadcast_address() {
        assert_eq!(
            broadcast_of("10.0.0.5", "255.255.255.252"),
            Some(Ipv4Addr::new(10, 0, 0, 7))
        );
    }
}
