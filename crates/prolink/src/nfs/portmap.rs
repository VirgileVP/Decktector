//! ONC-RPC portmapper (RFC 1057 Appendix A): resolves which port a program
//! (the MOUNT daemon, then the NFS daemon itself) actually listens on.
//! Neither is a fixed, well-known port — Deep Symmetry's `crate-digger`
//! reference implementation queries the portmapper fresh every time rather
//! than assuming a port, and this client does the same.

use super::rpc;
use super::xdr::{XdrDecoder, XdrEncoder};

pub const PMAP_PROGRAM: u32 = 100_000;
pub const PMAP_VERSION: u32 = 2;
const PMAP_PROC_GETPORT: u32 = 3;
/// `IPPROTO_UDP` — every call this crate's NFS client makes is UDP.
const PROTOCOL_UDP: u32 = 17;

/// Builds a `GETPORT` call asking which port `(program, version)` listens
/// on.
pub fn build_getport_call(xid: u32, program: u32, version: u32) -> Vec<u8> {
    let mut args = XdrEncoder::new();
    args.write_u32(program);
    args.write_u32(version);
    args.write_u32(PROTOCOL_UDP);
    args.write_u32(0); // The `port` field is only meaningful when *registering*; 0 when asking.
    rpc::build_call(
        xid,
        PMAP_PROGRAM,
        PMAP_VERSION,
        PMAP_PROC_GETPORT,
        &args.into_bytes(),
    )
}

/// The resolved port, or `None` if the reply doesn't match `expected_xid`,
/// wasn't accepted, or reports `0` — the portmapper's own way of saying
/// "nothing registered for that program/version."
pub fn parse_getport_reply(data: &[u8], expected_xid: u32) -> Option<u16> {
    let reply = rpc::parse_reply(data)?;
    if reply.xid != expected_xid || reply.accept_stat != rpc::ACCEPT_STAT_SUCCESS {
        return None;
    }
    let port = XdrDecoder::new(reply.results).read_u32()?;
    u16::try_from(port).ok().filter(|&port| port != 0)
}

/// Builds a `GETPORT` reply — exercised by this crate's tests' fake
/// portmapper, not by the real client.
pub fn build_getport_reply(xid: u32, port: u16) -> Vec<u8> {
    let mut results = XdrEncoder::new();
    results.write_u32(u32::from(port));
    rpc::build_accepted_reply(xid, rpc::ACCEPT_STAT_SUCCESS, &results.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_resolved_port_round_trips() {
        let reply = build_getport_reply(7, 51_413);
        assert_eq!(parse_getport_reply(&reply, 7), Some(51_413));
    }

    #[test]
    fn a_zero_port_means_not_registered() {
        let reply = build_getport_reply(7, 0);
        assert_eq!(parse_getport_reply(&reply, 7), None);
    }

    #[test]
    fn a_reply_for_a_different_call_is_rejected() {
        let reply = build_getport_reply(7, 111);
        assert_eq!(parse_getport_reply(&reply, 8), None);
    }
}
