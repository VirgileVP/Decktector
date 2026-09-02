//! The two NFSv2 (RFC 1094, program 100003 version 2) procedures this
//! client ever calls: `LOOKUP` (walk one path segment) and `READ` (fetch a
//! chunk of a file's bytes). `GETATTR` is never called separately —
//! `LOOKUP`'s reply already carries the attributes this client needs (type,
//! size) — and `READDIR` is never needed at all, since every path this
//! codebase reads is already known (`export.pdb`'s fixed location, or a
//! track's own `analyze_path` from its parsed database row), matching Deep
//! Symmetry's `crate-digger` reference implementation exactly.

use super::mount::{FileHandle, FILE_HANDLE_LEN};
use super::rpc;
use super::xdr::{XdrDecoder, XdrEncoder};

pub const NFS_PROGRAM: u32 = 100_003;
pub const NFS_VERSION: u32 = 2;
pub const NFSPROC_LOOKUP: u32 = 4;
pub const NFSPROC_READ: u32 = 6;

const NFS_OK: u32 = 0;
/// `ftype` value for a regular file (RFC 1094 §2.3.4) — what every file
/// this client fetches should report itself as.
pub const FTYPE_REGULAR_FILE: u32 = 1;
/// Protocol maximum per `READ` call; this client requests far less per call
/// (see [`super::client`]'s doc comment on why).
pub const NFS_MAXDATA: usize = 8192;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FileAttributes {
    pub file_type: u32,
    pub size: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LookupResult {
    pub handle: FileHandle,
    pub attributes: FileAttributes,
}

pub fn build_lookup_call(xid: u32, dir: FileHandle, name: &str) -> Vec<u8> {
    let mut args = XdrEncoder::new();
    args.write_opaque_fixed(&dir);
    args.write_string_utf16le(name);
    rpc::build_call(
        xid,
        NFS_PROGRAM,
        NFS_VERSION,
        NFSPROC_LOOKUP,
        &args.into_bytes(),
    )
}

/// The arguments of a `LOOKUP` call, as read back off the wire.
///
/// The client never needs this — it built the call itself. Reading a
/// recorded capture does: a reply carries only the handle it resolved to,
/// so the name that handle stands for is knowable only from the matching
/// call (see [`super::extract`]).
#[derive(Debug, Clone, PartialEq)]
pub struct LookupArgs {
    pub dir: FileHandle,
    pub name: String,
}

pub fn parse_lookup_call(args: &[u8]) -> Option<LookupArgs> {
    let mut decoder = XdrDecoder::new(args);
    let dir: FileHandle = decoder
        .read_opaque_fixed(FILE_HANDLE_LEN)?
        .try_into()
        .ok()?;
    let name = decoder.read_string_utf16le()?;
    Some(LookupArgs { dir, name })
}

pub fn parse_lookup_reply(data: &[u8], expected_xid: u32) -> Option<LookupResult> {
    let reply = rpc::parse_reply(data)?;
    if reply.xid != expected_xid || reply.accept_stat != rpc::ACCEPT_STAT_SUCCESS {
        return None;
    }
    let mut decoder = XdrDecoder::new(reply.results);
    if decoder.read_u32()? != NFS_OK {
        return None; // Most commonly NFSERR_NOENT — the path segment doesn't exist.
    }
    let handle: FileHandle = decoder
        .read_opaque_fixed(FILE_HANDLE_LEN)?
        .try_into()
        .ok()?;
    let attributes = read_fattr(&mut decoder)?;
    Some(LookupResult { handle, attributes })
}

pub fn build_lookup_reply(xid: u32, handle: FileHandle, attributes: FileAttributes) -> Vec<u8> {
    let mut results = XdrEncoder::new();
    results.write_u32(NFS_OK);
    results.write_opaque_fixed(&handle);
    write_fattr(&mut results, attributes);
    rpc::build_accepted_reply(xid, rpc::ACCEPT_STAT_SUCCESS, &results.into_bytes())
}

pub fn build_read_call(xid: u32, file: FileHandle, offset: u32, count: u32) -> Vec<u8> {
    let mut args = XdrEncoder::new();
    args.write_opaque_fixed(&file);
    args.write_u32(offset);
    args.write_u32(count);
    args.write_u32(0); // `totalcount`: present for historical reasons, unused by any server.
    rpc::build_call(
        xid,
        NFS_PROGRAM,
        NFS_VERSION,
        NFSPROC_READ,
        &args.into_bytes(),
    )
}

/// The arguments of a `READ` call, as read back off the wire — the file and
/// the offset a recorded reply's bytes belong at. See [`LookupArgs`] for why
/// only a capture reader needs this.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReadArgs {
    pub file: FileHandle,
    pub offset: u32,
    pub count: u32,
}

pub fn parse_read_call(args: &[u8]) -> Option<ReadArgs> {
    let mut decoder = XdrDecoder::new(args);
    let file: FileHandle = decoder
        .read_opaque_fixed(FILE_HANDLE_LEN)?
        .try_into()
        .ok()?;
    Some(ReadArgs {
        file,
        offset: decoder.read_u32()?,
        count: decoder.read_u32()?,
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReadResult {
    pub attributes: FileAttributes,
    pub data: Vec<u8>,
}

pub fn parse_read_reply(data: &[u8], expected_xid: u32) -> Option<ReadResult> {
    let reply = rpc::parse_reply(data)?;
    if reply.xid != expected_xid || reply.accept_stat != rpc::ACCEPT_STAT_SUCCESS {
        return None;
    }
    let mut decoder = XdrDecoder::new(reply.results);
    if decoder.read_u32()? != NFS_OK {
        return None;
    }
    let attributes = read_fattr(&mut decoder)?;
    let data = decoder.read_opaque_var()?.to_vec();
    Some(ReadResult { attributes, data })
}

pub fn build_read_reply(xid: u32, attributes: FileAttributes, data: &[u8]) -> Vec<u8> {
    let mut results = XdrEncoder::new();
    results.write_u32(NFS_OK);
    write_fattr(&mut results, attributes);
    results.write_opaque_var(data);
    rpc::build_accepted_reply(xid, rpc::ACCEPT_STAT_SUCCESS, &results.into_bytes())
}

/// `fattr` (RFC 1094 §2.3.5): 11 `u32` fields, then three `timeval`
/// (`{seconds, useconds}`, 8 bytes each) — 68 bytes total. Only `file_type`
/// and `size` carry weight for this client (mirrors `crate-digger`, which
/// reads exactly these two off the same reply and nothing else); the rest
/// still has to be walked correctly to stay aligned with whatever follows.
fn read_fattr(decoder: &mut XdrDecoder<'_>) -> Option<FileAttributes> {
    let file_type = decoder.read_u32()?;
    let _mode = decoder.read_u32()?;
    let _nlink = decoder.read_u32()?;
    let _uid = decoder.read_u32()?;
    let _gid = decoder.read_u32()?;
    let size = decoder.read_u32()?;
    let _blocksize = decoder.read_u32()?;
    let _rdev = decoder.read_u32()?;
    let _blocks = decoder.read_u32()?;
    let _fsid = decoder.read_u32()?;
    let _fileid = decoder.read_u32()?;
    for _ in 0..3 {
        // atime, mtime, ctime.
        decoder.read_u32()?;
        decoder.read_u32()?;
    }
    Some(FileAttributes { file_type, size })
}

fn write_fattr(encoder: &mut XdrEncoder, attributes: FileAttributes) {
    encoder.write_u32(attributes.file_type);
    for _ in 0..4 {
        encoder.write_u32(0); // mode, nlink, uid, gid — not read back by this client.
    }
    encoder.write_u32(attributes.size);
    for _ in 0..5 {
        encoder.write_u32(0); // blocksize, rdev, blocks, fsid, fileid.
    }
    for _ in 0..6 {
        encoder.write_u32(0); // atime, mtime, ctime (2 words each).
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn handle(fill: u8) -> FileHandle {
        [fill; FILE_HANDLE_LEN]
    }

    #[test]
    fn a_lookup_result_round_trips() {
        let attributes = FileAttributes {
            file_type: FTYPE_REGULAR_FILE,
            size: 123_456,
        };
        let reply = build_lookup_reply(5, handle(0x11), attributes);
        let result = parse_lookup_reply(&reply, 5).expect("should parse");

        assert_eq!(result.handle, handle(0x11));
        assert_eq!(result.attributes, attributes);
    }

    #[test]
    fn a_lookup_for_a_missing_segment_is_none() {
        // NFSERR_NOENT (2) instead of NFS_OK.
        let reply = rpc::build_accepted_reply(5, rpc::ACCEPT_STAT_SUCCESS, &2u32.to_be_bytes());
        assert!(parse_lookup_reply(&reply, 5).is_none());
    }

    #[test]
    fn a_read_result_round_trips() {
        let attributes = FileAttributes {
            file_type: FTYPE_REGULAR_FILE,
            size: 5,
        };
        let reply = build_read_reply(9, attributes, &[1, 2, 3, 4, 5]);
        let result = parse_read_reply(&reply, 9).expect("should parse");

        assert_eq!(result.attributes, attributes);
        assert_eq!(result.data, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn a_read_call_names_the_requested_offset_and_count() {
        let bytes = build_read_call(1, handle(0), 2048, 2048);
        // Header(40) + handle(32) + offset(4) + count(4) + totalcount(4).
        let offset_field = &bytes[40 + 32..40 + 32 + 4];
        assert_eq!(u32::from_be_bytes(offset_field.try_into().unwrap()), 2048);
    }

    #[test]
    fn a_reply_for_a_different_call_is_rejected() {
        let attributes = FileAttributes {
            file_type: FTYPE_REGULAR_FILE,
            size: 1,
        };
        let reply = build_lookup_reply(5, handle(0), attributes);
        assert!(parse_lookup_reply(&reply, 6).is_none());
    }
}
