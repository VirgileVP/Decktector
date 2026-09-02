//! Rebuilds the files an NFS conversation carried, out of a recording of
//! that conversation.
//!
//! On a linked booth the players read each other's media over this
//! protocol, so a capture of the segment already contains what a live
//! [`super::client`] session would have had to fetch: `export.pdb`, and the
//! `.DAT`/`.EXT` analysis files holding a track's beat grid, cues and
//! waveform. Recovering them offline turns one recording into the input for
//! every parser in this crate — and, past that, into real track fixtures for
//! the simulator, whose generated waveforms are otherwise plausible rather
//! than true.
//!
//! The reassembly is deliberately forgiving. A capture misses packets, is
//! started mid-transfer, and interleaves several files across several
//! clients; none of that may abort the run. Whatever bytes did cross the
//! wire are recovered, and every gap is reported instead of being papered
//! over — a half-read file handed to a parser as if it were whole is worse
//! than no file at all.
//!
//! What this cannot do: recover a file whose `LOOKUP` chain the capture
//! missed. The bytes are still reassembled, but the name is not in them —
//! only the calls that resolved the handle carry it — so such a file is
//! reported under its handle instead (see [`ExtractedFile::path`]).

use std::collections::HashMap;
use std::net::Ipv4Addr;

use super::mount::FileHandle;
use super::procedures::{self, LookupArgs, ReadArgs};
use super::rpc;

/// Cap on a single reassembled file, so a corrupt offset in a capture
/// cannot make this allocate without bound. Comfortably past any
/// `export.pdb` a real USB carries.
const MAX_FILE_LEN: usize = 512 * 1024 * 1024;

/// A file recovered from a capture.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtractedFile {
    /// Player the file was read from.
    pub server: Ipv4Addr,
    /// Slash-separated path, rebuilt from the `LOOKUP` calls that resolved
    /// this file's handle. Falls back to `handle-<hex>` when the capture
    /// does not contain those calls.
    pub path: String,
    /// Size the server reported, when a reply carried it.
    pub reported_size: Option<u32>,
    pub bytes: Vec<u8>,
    /// Byte ranges no `READ` reply in the capture covered. Empty means the
    /// file is whole up to its reported size.
    pub gaps: Vec<(usize, usize)>,
}

impl ExtractedFile {
    /// Whether every byte up to the reported size was recovered. A file the
    /// server never reported a size for can only be judged by its gaps.
    pub fn is_complete(&self) -> bool {
        self.gaps.is_empty()
            && self
                .reported_size
                .is_none_or(|size| self.bytes.len() as u64 >= u64::from(size))
    }
}

/// What a `READ` or `LOOKUP` reply, when it arrives, will turn out to be
/// about. A reply carries only its transaction id — the handle, the offset
/// and the name all live in the call.
#[derive(Debug, Clone)]
enum PendingCall {
    Lookup(LookupArgs),
    Read(ReadArgs),
}

/// A file's bytes as they accumulate, indexed by the ranges actually seen so
/// that gaps stay visible.
#[derive(Debug, Default)]
struct PartialFile {
    reported_size: Option<u32>,
    bytes: Vec<u8>,
    filled: Vec<(usize, usize)>,
}

impl PartialFile {
    fn insert(&mut self, offset: usize, data: &[u8]) {
        let end = offset.saturating_add(data.len());
        if end > MAX_FILE_LEN {
            return;
        }
        if self.bytes.len() < end {
            self.bytes.resize(end, 0);
        }
        self.bytes[offset..end].copy_from_slice(data);
        self.filled.push((offset, end));
        self.filled.sort_unstable();
        let mut coalesced: Vec<(usize, usize)> = Vec::with_capacity(self.filled.len());
        for &(start, end) in &self.filled {
            match coalesced.last_mut() {
                Some(last) if start <= last.1 => last.1 = last.1.max(end),
                _ => coalesced.push((start, end)),
            }
        }
        self.filled = coalesced;
    }

    /// Ranges between the filled ones, plus the tail missing before the
    /// reported size.
    fn gaps(&self) -> Vec<(usize, usize)> {
        let mut gaps = Vec::new();
        let mut cursor = 0;
        for &(start, end) in &self.filled {
            if start > cursor {
                gaps.push((cursor, start));
            }
            cursor = cursor.max(end);
        }
        if let Some(size) = self.reported_size {
            let size = size as usize;
            if cursor < size {
                gaps.push((cursor, size));
            }
        }
        gaps
    }
}

/// Counts describing how much of a capture this made sense of, for a caller
/// to report rather than imply.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExtractStats {
    pub lookup_calls: u64,
    pub read_calls: u64,
    pub read_replies: u64,
    /// Replies whose matching call is not in the capture — normal at the
    /// very start of a recording, a sign of packet loss anywhere else.
    pub replies_without_a_call: u64,
    /// Datagrams on the NFS port that were not NFS at all, or could not be
    /// decoded.
    pub undecodable: u64,
}

/// Accumulates NFS traffic, datagram by datagram, into whole files.
///
/// Fed from any source that can supply payloads and their direction — a
/// pcapng import, a `prolink-capture` session's `udp.jsonl`, or a live
/// listener — since none of the reassembly depends on where the bytes came
/// from.
#[derive(Debug, Default)]
pub struct NfsExtractor {
    /// Keyed by transaction id. A reply matches the one call that carried
    /// the same id; a stale entry costs one small allocation and is
    /// overwritten when the id is reused.
    pending: HashMap<u32, PendingCall>,
    /// Handle to (parent handle, name), from every `LOOKUP` that resolved,
    /// which is what makes a path rebuildable.
    names: HashMap<(Ipv4Addr, FileHandle), (FileHandle, String)>,
    files: HashMap<(Ipv4Addr, FileHandle), PartialFile>,
    stats: ExtractStats,
}

impl NfsExtractor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn stats(&self) -> ExtractStats {
        self.stats
    }

    /// Feeds one datagram sent *to* an NFS server: a client's call.
    pub fn observe_call(&mut self, payload: &[u8]) {
        let Some(call) = rpc::parse_call(payload) else {
            self.stats.undecodable += 1;
            return;
        };
        if call.prog != procedures::NFS_PROGRAM {
            return;
        }
        match call.proc {
            procedures::NFSPROC_LOOKUP => match procedures::parse_lookup_call(call.args) {
                Some(args) => {
                    self.stats.lookup_calls += 1;
                    self.pending.insert(call.xid, PendingCall::Lookup(args));
                }
                None => self.stats.undecodable += 1,
            },
            procedures::NFSPROC_READ => match procedures::parse_read_call(call.args) {
                Some(args) => {
                    self.stats.read_calls += 1;
                    self.pending.insert(call.xid, PendingCall::Read(args));
                }
                None => self.stats.undecodable += 1,
            },
            // GETATTR, READDIR and the rest: this crate never calls them,
            // and nothing here needs what they return.
            _ => {}
        }
    }

    /// Feeds one datagram sent *by* an NFS server, `server` being its
    /// address — the key everything recovered is filed under, since two
    /// players hand out unrelated handles from the same value space.
    pub fn observe_reply(&mut self, server: Ipv4Addr, payload: &[u8]) {
        let Some(reply) = rpc::parse_reply(payload) else {
            self.stats.undecodable += 1;
            return;
        };
        let Some(pending) = self.pending.remove(&reply.xid) else {
            self.stats.replies_without_a_call += 1;
            return;
        };
        match pending {
            PendingCall::Lookup(args) => self.record_lookup(server, &args, payload, reply.xid),
            PendingCall::Read(args) => self.record_read(server, args, payload, reply.xid),
        }
    }

    fn record_lookup(&mut self, server: Ipv4Addr, args: &LookupArgs, payload: &[u8], xid: u32) {
        // A failed lookup (most often a path segment that doesn't exist)
        // parses as `None` and simply teaches this nothing.
        let Some(result) = procedures::parse_lookup_reply(payload, xid) else {
            return;
        };
        self.names
            .insert((server, result.handle), (args.dir, args.name.clone()));
    }

    fn record_read(&mut self, server: Ipv4Addr, args: ReadArgs, payload: &[u8], xid: u32) {
        let Some(result) = procedures::parse_read_reply(payload, xid) else {
            return;
        };
        self.stats.read_replies += 1;
        let file = self.files.entry((server, args.file)).or_default();
        file.reported_size = Some(result.attributes.size);
        file.insert(args.offset as usize, &result.data);
    }

    /// Every file the capture carried bytes for, ordered by path so a run
    /// over the same capture reports the same thing twice.
    pub fn into_files(mut self) -> Vec<ExtractedFile> {
        let keys: Vec<(Ipv4Addr, FileHandle)> = self.files.keys().copied().collect();
        let mut files: Vec<ExtractedFile> = keys
            .into_iter()
            .filter_map(|key| {
                let path = self.path_of(key);
                let partial = self.files.remove(&key)?;
                Some(ExtractedFile {
                    server: key.0,
                    path,
                    reported_size: partial.reported_size,
                    gaps: partial.gaps(),
                    bytes: partial.bytes,
                })
            })
            .collect();
        files.sort_by(|left, right| (left.server, &left.path).cmp(&(right.server, &right.path)));
        files
    }

    /// Walks a handle back up its `LOOKUP` chain to build a path. Stops at
    /// the first handle no lookup in the capture resolved — usually the
    /// mount root, which is exactly where a path should stop.
    fn path_of(&self, key: (Ipv4Addr, FileHandle)) -> String {
        let (server, handle) = key;
        let mut segments = Vec::new();
        let mut current = handle;
        // A malformed capture could in principle describe a cycle; the
        // visited set makes that terminate rather than hang.
        let mut visited = Vec::new();
        while let Some((parent, name)) = self.names.get(&(server, current)) {
            if visited.contains(&current) {
                break;
            }
            visited.push(current);
            segments.push(name.clone());
            current = *parent;
        }
        if segments.is_empty() {
            return format!("handle-{}", hex(&handle));
        }
        segments.reverse();
        segments.join("/")
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nfs::mount::FILE_HANDLE_LEN;
    use crate::nfs::procedures::{
        build_lookup_call, build_lookup_reply, build_read_call, build_read_reply, FileAttributes,
        FTYPE_REGULAR_FILE,
    };

    const SERVER: Ipv4Addr = Ipv4Addr::new(169, 254, 7, 50);
    const ROOT: FileHandle = [0x00; FILE_HANDLE_LEN];

    fn handle(fill: u8) -> FileHandle {
        [fill; FILE_HANDLE_LEN]
    }

    fn attributes(size: u32) -> FileAttributes {
        FileAttributes {
            file_type: FTYPE_REGULAR_FILE,
            size,
        }
    }

    /// Walks one path segment: the call names it, the reply resolves it.
    fn lookup(
        extractor: &mut NfsExtractor,
        xid: u32,
        dir: FileHandle,
        name: &str,
        resolved: FileHandle,
    ) {
        extractor.observe_call(&build_lookup_call(xid, dir, name));
        extractor.observe_reply(SERVER, &build_lookup_reply(xid, resolved, attributes(0)));
    }

    fn read(
        extractor: &mut NfsExtractor,
        xid: u32,
        file: FileHandle,
        offset: u32,
        size: u32,
        data: &[u8],
    ) {
        extractor.observe_call(&build_read_call(xid, file, offset, data.len() as u32));
        extractor.observe_reply(SERVER, &build_read_reply(xid, attributes(size), data));
    }

    #[test]
    fn rebuilds_a_file_and_its_path() {
        let mut extractor = NfsExtractor::new();
        lookup(&mut extractor, 1, ROOT, "PIONEER", handle(0x11));
        lookup(&mut extractor, 2, handle(0x11), "rekordbox", handle(0x22));
        lookup(&mut extractor, 3, handle(0x22), "export.pdb", handle(0x33));
        read(&mut extractor, 4, handle(0x33), 0, 8, b"abcd");
        read(&mut extractor, 5, handle(0x33), 4, 8, b"efgh");

        let files = extractor.into_files();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "PIONEER/rekordbox/export.pdb");
        assert_eq!(files[0].bytes, b"abcdefgh".to_vec());
        assert_eq!(files[0].reported_size, Some(8));
        assert!(files[0].is_complete());
    }

    /// Chunks are requested in parallel and may be answered out of order.
    #[test]
    fn accepts_chunks_out_of_order() {
        let mut extractor = NfsExtractor::new();
        lookup(&mut extractor, 1, ROOT, "ANLZ0000.EXT", handle(0x44));
        read(&mut extractor, 3, handle(0x44), 4, 8, b"efgh");
        read(&mut extractor, 2, handle(0x44), 0, 8, b"abcd");

        let files = extractor.into_files();
        assert_eq!(files[0].bytes, b"abcdefgh".to_vec());
        assert!(files[0].gaps.is_empty());
    }

    /// A capture that lost a reply must say so rather than hand a parser a
    /// file with four zero bytes in the middle of it.
    #[test]
    fn reports_a_missing_chunk_as_a_gap() {
        let mut extractor = NfsExtractor::new();
        lookup(&mut extractor, 1, ROOT, "export.pdb", handle(0x55));
        read(&mut extractor, 2, handle(0x55), 0, 12, b"abcd");
        read(&mut extractor, 3, handle(0x55), 8, 12, b"ijkl");

        let files = extractor.into_files();
        assert_eq!(files[0].gaps, vec![(4, 8)]);
        assert!(!files[0].is_complete());
    }

    /// Bytes short of the size the server reported are a gap too, even
    /// though nothing follows them.
    #[test]
    fn reports_a_missing_tail_as_a_gap() {
        let mut extractor = NfsExtractor::new();
        lookup(&mut extractor, 1, ROOT, "export.pdb", handle(0x66));
        read(&mut extractor, 2, handle(0x66), 0, 16, b"abcd");

        let files = extractor.into_files();
        assert_eq!(files[0].gaps, vec![(4, 16)]);
        assert!(!files[0].is_complete());
    }

    /// A recording started mid-transfer still recovers the bytes; only the
    /// name is unrecoverable, since it lives in calls that were never
    /// captured.
    #[test]
    fn names_a_file_by_its_handle_when_the_lookup_was_missed() {
        let mut extractor = NfsExtractor::new();
        read(&mut extractor, 1, handle(0xAB), 0, 4, b"data");

        let files = extractor.into_files();
        assert!(
            files[0].path.starts_with("handle-ab"),
            "unexpected path {}",
            files[0].path
        );
        assert_eq!(files[0].bytes, b"data".to_vec());
    }

    /// A reply with no call in the capture is counted, not guessed at.
    #[test]
    fn counts_a_reply_with_no_matching_call() {
        let mut extractor = NfsExtractor::new();
        extractor.observe_reply(SERVER, &build_read_reply(99, attributes(4), b"data"));

        assert_eq!(extractor.stats().replies_without_a_call, 1);
        assert!(extractor.into_files().is_empty());
    }

    /// Two players hand out handles from the same value space; the same
    /// handle on two servers is two different files.
    #[test]
    fn keeps_identical_handles_from_two_servers_apart() {
        let other = Ipv4Addr::new(169, 254, 6, 92);
        let mut extractor = NfsExtractor::new();
        extractor.observe_call(&build_read_call(1, handle(0x77), 0, 4));
        extractor.observe_reply(SERVER, &build_read_reply(1, attributes(4), b"aaaa"));
        extractor.observe_call(&build_read_call(2, handle(0x77), 0, 4));
        extractor.observe_reply(other, &build_read_reply(2, attributes(4), b"bbbb"));

        let files = extractor.into_files();
        assert_eq!(files.len(), 2);
        assert_eq!(
            files.iter().map(|file| file.server).collect::<Vec<_>>(),
            vec![other, SERVER]
        );
    }
}
