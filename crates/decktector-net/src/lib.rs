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

/// The IPv4 address of the named interface, to bind a socket or listener to.
///
/// Re-resolved on every config change rather than cached indefinitely: an
/// adapter can pick up a new address (DHCP) or disappear (unplugged) between
/// one set and the next.
pub fn resolve_ipv4(interface_name: &str) -> anyhow::Result<Ipv4Addr> {
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
                if_addrs::IfAddr::V4(v4) => Some(v4.ip),
                if_addrs::IfAddr::V6(_) => None,
            }
        })
        .ok_or_else(|| anyhow::anyhow!("network interface '{interface_name}' has no IPv4 address"))
}
