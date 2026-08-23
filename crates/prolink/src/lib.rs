//! Pro DJ Link protocol client.
//!
//! Speaks the wire format of Pioneer/AlphaTheta CDJ and XDJ players: announces
//! a virtual CDJ, discovers devices, reads player status and beat packets, and
//! fetches track metadata. Nothing here knows about Decktective's domain model
//! — that mapping lives in `decktective-core`.
//!
//! The protocol is not documented by the manufacturer. This implementation
//! follows Deep Symmetry's analysis at <https://djl-analysis.deepsymmetry.org/>.
//!
//! Landing in phases: the UDP layer (announce, discovery, status, beat) is
//! done; metadata over NFS and the remote database fallback are next.

pub mod beat;
pub mod capture;
pub mod dbserver;
pub mod discovery;
pub mod nfs;
pub mod pcap;
pub mod presence;
pub mod status;

use socket2::{Domain, Socket, Type};
use std::net::{Ipv4Addr, SocketAddr};
use tokio::net::UdpSocket;

/// UDP port players announce themselves on.
pub const ANNOUNCE_PORT: u16 = 50000;
/// UDP port carrying beat and precise-position packets.
pub const BEAT_PORT: u16 = 50001;
/// UDP port carrying player status packets.
pub const STATUS_PORT: u16 = 50002;

/// Magic header every Pro DJ Link packet starts with.
pub const PROLINK_HEADER: [u8; 10] = [0x51, 0x73, 0x70, 0x74, 0x31, 0x57, 0x6d, 0x4a, 0x4f, 0x4c];

/// Encoding shared by every pitch/tempo-adjustment field in the protocol
/// (status, beat and precise-position packets alike): a 32-bit big-endian
/// fixed-point value where this raw value is unity (0% adjustment) and the
/// scale is linear from there — e.g. `0x00180000` is +50%.
const PITCH_UNITY: u32 = 0x0010_0000;

/// Decodes a raw 32-bit pitch field into a percentage, e.g. `2.35` for +2.35%.
pub(crate) fn decode_pitch(raw: u32) -> f64 {
    (f64::from(raw) / f64::from(PITCH_UNITY) - 1.0) * 100.0
}

/// Binds a UDP socket for Pro DJ Link traffic on `port`: `SO_REUSEADDR` (and,
/// on Unix, `SO_REUSEPORT`) so this listener can coexist with rekordbox or
/// another Pro DJ Link tool already bound to the same port on this machine,
/// broadcast-capable since several packet types are broadcast rather than
/// unicast, and non-blocking for use with Tokio.
pub(crate) fn bind_udp_socket(port: u16) -> anyhow::Result<UdpSocket> {
    bind_udp_socket_on(Ipv4Addr::UNSPECIFIED, port)
}

/// [`bind_udp_socket`] on one specific local address, for sending rather
/// than listening: a broadcast leaving a multi-homed machine has to be
/// pinned to the booth's adapter, and the port it leaves *from* matters too
/// — real players send their keep-alives from [`ANNOUNCE_PORT`] itself, and
/// that is the shape peers on the network are known to accept.
///
/// Binding a specific address is why the wildcard listeners' `SO_REUSEADDR`
/// / `SO_REUSEPORT` are load-bearing here: this socket shares
/// [`ANNOUNCE_PORT`] with whichever listener is already on it.
pub(crate) fn bind_udp_socket_on(ip: Ipv4Addr, port: u16) -> anyhow::Result<UdpSocket> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, None)?;
    socket.set_reuse_address(true)?;
    #[cfg(unix)]
    socket.set_reuse_port(true)?;
    socket.set_broadcast(true)?;
    socket.set_nonblocking(true)?;
    socket.bind(&SocketAddr::from((ip, port)).into())?;
    Ok(UdpSocket::from_std(socket.into())?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_is_the_qspt1_wmjol_magic() {
        assert_eq!(&PROLINK_HEADER[..5], b"Qspt1");
    }

    #[test]
    fn decode_pitch_reads_unity_as_zero_percent() {
        assert_eq!(decode_pitch(PITCH_UNITY), 0.0);
    }

    #[test]
    fn decode_pitch_reads_a_positive_adjustment() {
        // 0x00180000 / 0x00100000 = 1.5 -> +50%.
        assert_eq!(decode_pitch(0x0018_0000), 50.0);
    }

    #[test]
    fn decode_pitch_reads_a_negative_adjustment() {
        // 0x000c0000 / 0x00100000 = 0.75 -> -25%.
        assert_eq!(decode_pitch(0x000c_0000), -25.0);
    }
}
