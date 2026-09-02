//! ONC-RPC (RFC 1057) call/reply envelope — the wrapper every portmapper,
//! MOUNT and NFS message in this module rides inside. Procedure-specific
//! arguments/results are opaque to this layer; callers append/consume them
//! before/after this envelope using [`super::xdr`].
//!
//! Every call here uses `AUTH_NONE` credentials — Pro DJ Link has no
//! authentication anywhere else in the protocol family either (the UDP
//! discovery/status/beat traffic and the dbserver TCP protocol are both
//! wide open to anyone on the network), so there's no reason to expect this
//! corner of it to be different, and Deep Symmetry's `crate-digger`
//! reference implementation does the same.

use super::xdr::{XdrDecoder, XdrEncoder};

const RPC_VERSION: u32 = 2;
const MSG_TYPE_CALL: u32 = 0;
const MSG_TYPE_REPLY: u32 = 1;
const REPLY_STAT_MSG_ACCEPTED: u32 = 0;
const AUTH_FLAVOR_NONE: u32 = 0;

/// `accept_stat` value meaning the call actually ran and the results follow.
pub const ACCEPT_STAT_SUCCESS: u32 = 0;

/// Builds a complete call message: the RPC envelope followed by
/// `procedure_args` (already XDR-encoded by the caller) — everything a UDP
/// packet to the portmapper, MOUNT or NFS daemon needs.
pub fn build_call(xid: u32, prog: u32, vers: u32, proc: u32, procedure_args: &[u8]) -> Vec<u8> {
    let mut encoder = XdrEncoder::new();
    encoder.write_u32(xid);
    encoder.write_u32(MSG_TYPE_CALL);
    encoder.write_u32(RPC_VERSION);
    encoder.write_u32(prog);
    encoder.write_u32(vers);
    encoder.write_u32(proc);
    write_auth_none(&mut encoder); // cred
    write_auth_none(&mut encoder); // verf
    let mut bytes = encoder.into_bytes();
    bytes.extend_from_slice(procedure_args);
    bytes
}

/// A parsed call envelope — only used by this crate's own tests' fake
/// servers (the real client only ever builds calls, via [`build_call`],
/// never parses them), kept here rather than test-local so it stays next
/// to the format it mirrors.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RpcCall<'a> {
    pub xid: u32,
    pub prog: u32,
    pub vers: u32,
    pub proc: u32,
    pub args: &'a [u8],
}

pub fn parse_call(data: &[u8]) -> Option<RpcCall<'_>> {
    let mut decoder = XdrDecoder::new(data);
    let xid = decoder.read_u32()?;
    if decoder.read_u32()? != MSG_TYPE_CALL {
        return None;
    }
    if decoder.read_u32()? != RPC_VERSION {
        return None;
    }
    let prog = decoder.read_u32()?;
    let vers = decoder.read_u32()?;
    let proc = decoder.read_u32()?;
    read_auth_none_ish(&mut decoder)?; // cred
    read_auth_none_ish(&mut decoder)?; // verf
    Some(RpcCall {
        xid,
        prog,
        vers,
        proc,
        args: decoder.remaining(),
    })
}

/// A parsed reply envelope. `results` is whatever bytes followed the
/// envelope — the procedure-specific reply — meaningful only when
/// `accept_stat == `[`ACCEPT_STAT_SUCCESS`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RpcReply<'a> {
    pub xid: u32,
    pub accept_stat: u32,
    pub results: &'a [u8],
}

/// Parses a reply envelope; `None` if the bytes are too short, aren't a
/// reply, or the server rejected the message outright (a real but distinct
/// failure from "not enough data yet" — see [`super::xdr`]'s doc comment on
/// the same ambiguity elsewhere in this crate). A message the server
/// *accepted* but couldn't fully service (bad program/version/procedure) is
/// still returned — `accept_stat` carries that, since the caller is
/// generally better placed to decide whether e.g. `PROC_UNAVAIL` is worth
/// retrying differently than a bare timeout.
pub fn parse_reply(data: &[u8]) -> Option<RpcReply<'_>> {
    let mut decoder = XdrDecoder::new(data);
    let xid = decoder.read_u32()?;
    if decoder.read_u32()? != MSG_TYPE_REPLY {
        return None;
    }
    if decoder.read_u32()? != REPLY_STAT_MSG_ACCEPTED {
        return None; // MSG_DENIED — the server refused the call's auth entirely.
    }
    read_auth_none_ish(&mut decoder)?; // verf — flavor/body not inspected, AUTH_NONE expected back.
    let accept_stat = decoder.read_u32()?;
    Some(RpcReply {
        xid,
        accept_stat,
        results: decoder.remaining(),
    })
}

/// Builds a complete accepted-reply envelope followed by `results` —
/// exercised by this crate's own tests' fake UDP server, not by the real
/// client (which only ever calls, never replies), but kept here so the
/// encode/decode pair stays next to each other like every other parser in
/// this crate.
pub fn build_accepted_reply(xid: u32, accept_stat: u32, results: &[u8]) -> Vec<u8> {
    let mut encoder = XdrEncoder::new();
    encoder.write_u32(xid);
    encoder.write_u32(MSG_TYPE_REPLY);
    encoder.write_u32(REPLY_STAT_MSG_ACCEPTED);
    write_auth_none(&mut encoder); // verf
    encoder.write_u32(accept_stat);
    let mut bytes = encoder.into_bytes();
    bytes.extend_from_slice(results);
    bytes
}

fn write_auth_none(encoder: &mut XdrEncoder) {
    encoder.write_u32(AUTH_FLAVOR_NONE);
    encoder.write_opaque_var(&[]);
}

/// Reads an `opaque_auth` without asserting its flavor is exactly
/// `AUTH_NONE` — a real player's verifier is whatever it is; this client
/// only needs to skip over it correctly, not validate it.
fn read_auth_none_ish(decoder: &mut XdrDecoder<'_>) -> Option<()> {
    let _flavor = decoder.read_u32()?;
    let _body = decoder.read_opaque_var()?;
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_call_and_accepted_reply_round_trip() {
        let call = build_call(42, 100_003, 2, 4, &[9, 9, 9, 9]);
        // A real server would decode `call` and respond; simulate the
        // response directly to test the reply side.
        let reply_bytes = build_accepted_reply(42, ACCEPT_STAT_SUCCESS, &[1, 2, 3, 4]);
        let reply = parse_reply(&reply_bytes).expect("should parse");

        assert_eq!(reply.xid, 42);
        assert_eq!(reply.accept_stat, ACCEPT_STAT_SUCCESS);
        assert_eq!(reply.results, &[1, 2, 3, 4]);
        assert!(
            call.len() > 24,
            "the call envelope plus args should be well past the bare header size"
        );
    }

    #[test]
    fn a_non_success_accept_stat_is_reported_not_hidden() {
        let reply_bytes = build_accepted_reply(1, 3 /* PROC_UNAVAIL */, &[]);
        let reply = parse_reply(&reply_bytes).expect("should still parse the envelope");
        assert_eq!(reply.accept_stat, 3);
        assert!(reply.results.is_empty());
    }

    #[test]
    fn a_call_message_type_is_not_accepted_as_a_reply() {
        let call = build_call(1, 100_005, 1, 1, &[]);
        assert!(parse_reply(&call).is_none());
    }

    #[test]
    fn a_truncated_reply_is_rejected_rather_than_panicking() {
        let reply_bytes = build_accepted_reply(1, ACCEPT_STAT_SUCCESS, &[1, 2, 3, 4]);
        assert!(parse_reply(&reply_bytes[..8]).is_none());
    }

    #[test]
    fn a_call_is_parsed_back_into_its_program_version_procedure_and_args() {
        let bytes = build_call(11, 100_005, 1, 1, &[9, 9, 9, 9]);
        let call = parse_call(&bytes).expect("should parse");
        assert_eq!(call.xid, 11);
        assert_eq!(call.prog, 100_005);
        assert_eq!(call.vers, 1);
        assert_eq!(call.proc, 1);
        assert_eq!(call.args, &[9, 9, 9, 9]);
    }

    #[test]
    fn the_xid_round_trips_so_a_caller_can_match_replies_to_calls() {
        let reply_bytes = build_accepted_reply(0xabcd_1234, ACCEPT_STAT_SUCCESS, &[]);
        assert_eq!(parse_reply(&reply_bytes).unwrap().xid, 0xabcd_1234);
    }
}
