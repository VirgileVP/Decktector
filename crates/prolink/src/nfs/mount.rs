//! MOUNT protocol (RFC 1094 Appendix A): exchanges a slot's export path
//! (e.g. `"/C/"`) for the NFS root file handle every subsequent
//! [`super::procedures::lookup`] call chains from.

use super::rpc;
use super::xdr::{XdrDecoder, XdrEncoder};

pub const MOUNT_PROGRAM: u32 = 100_005;
pub const MOUNT_VERSION: u32 = 1;
const MOUNTPROC_MNT: u32 = 1;

/// NFSv2 file handles are always exactly this many opaque bytes.
pub const FILE_HANDLE_LEN: usize = 32;
pub type FileHandle = [u8; FILE_HANDLE_LEN];

/// Builds an `MNT` call for `dir_path` — encoded UTF-16LE, the one wire
/// deviation from the RFC (see [`super::xdr`]'s doc comment).
pub fn build_mnt_call(xid: u32, dir_path: &str) -> Vec<u8> {
    let mut args = XdrEncoder::new();
    args.write_string_utf16le(dir_path);
    rpc::build_call(xid, MOUNT_PROGRAM, MOUNT_VERSION, MOUNTPROC_MNT, &args.into_bytes())
}

/// The root file handle, or `None` if the reply doesn't match, wasn't
/// accepted, or the mount itself failed (a non-zero `fhstatus` — the path
/// isn't exported, e.g. an empty/missing media slot).
pub fn parse_mnt_reply(data: &[u8], expected_xid: u32) -> Option<FileHandle> {
    let reply = rpc::parse_reply(data)?;
    if reply.xid != expected_xid || reply.accept_stat != rpc::ACCEPT_STAT_SUCCESS {
        return None;
    }
    let mut decoder = XdrDecoder::new(reply.results);
    if decoder.read_u32()? != 0 {
        return None; // Non-zero fhstatus: mount failed.
    }
    decoder.read_opaque_fixed(FILE_HANDLE_LEN)?.try_into().ok()
}

/// Builds an `MNT` reply — exercised by this crate's tests' fake MOUNT
/// daemon, not by the real client.
pub fn build_mnt_reply(xid: u32, handle: FileHandle) -> Vec<u8> {
    let mut results = XdrEncoder::new();
    results.write_u32(0); // fhstatus: success.
    results.write_opaque_fixed(&handle);
    rpc::build_accepted_reply(xid, rpc::ACCEPT_STAT_SUCCESS, &results.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn handle(fill: u8) -> FileHandle {
        [fill; FILE_HANDLE_LEN]
    }

    #[test]
    fn a_root_handle_round_trips() {
        let reply = build_mnt_reply(3, handle(0xaa));
        assert_eq!(parse_mnt_reply(&reply, 3), Some(handle(0xaa)));
    }

    #[test]
    fn a_reply_for_a_different_call_is_rejected() {
        let reply = build_mnt_reply(3, handle(0xaa));
        assert_eq!(parse_mnt_reply(&reply, 4), None);
    }

    #[test]
    fn the_mount_path_is_utf16le_encoded_on_the_wire() {
        let bytes = build_mnt_call(1, "C");
        // Header (28) + 4-byte length + 2 UTF-16LE bytes for 'C' + 2 padding.
        assert_eq!(&bytes[bytes.len() - 4..], &[b'C', 0x00, 0x00, 0x00]);
    }
}
