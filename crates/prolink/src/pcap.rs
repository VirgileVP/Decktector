//! Reads UDP datagrams back out of a pcapng file, so a Wireshark recording
//! of a booth network can feed the same offline tooling a
//! [`crate::capture`] session does.
//!
//! Why this exists alongside `prolink-capture`: the runbook asks for a
//! Wireshark recording on every hardware session precisely because it
//! survives the capture tool being wrong. The 2026-08-19 session proved
//! that risk real — a passive `prolink-capture` run recorded no status at
//! all — while a `.pcapng` taken next to it keeps every byte the network
//! carried, including traffic other devices exchanged between themselves
//! that no single listener socket would ever have been sent.
//!
//! Hand-rolled rather than pulled from a pcap crate, for the same reason
//! [`crate::nfs::xdr`] and [`crate::nfs::pdb`] are hand-rolled: the format
//! is a short, stable, well-documented block walk, and the part that is
//! actually load-bearing here — IPv4 fragment reassembly, without which
//! every NFS read in a capture is unreadable — is not something those
//! crates do anyway.
//!
//! Scope is UDP over IPv4 over Ethernet, which is all Pro DJ Link and its
//! NFS metadata route ever use. A TCP stream (the dbserver route) would
//! need sequence-number reassembly this module deliberately does not
//! attempt.

use std::collections::HashMap;
use std::io::{self, Read};
use std::net::Ipv4Addr;

/// pcapng block types this module reacts to; every other type is skipped by
/// its declared length. See PCAP Next Generation Capture File Format,
/// section 4.
const BLOCK_SECTION_HEADER: u32 = 0x0A0D_0D0A;
const BLOCK_INTERFACE_DESCRIPTION: u32 = 0x0000_0001;
const BLOCK_ENHANCED_PACKET: u32 = 0x0000_0006;

/// Byte-order magic in a section header block, read little-endian. Reading
/// it big-endian instead yields the byte-swapped form, and that is the
/// whole mechanism by which a section declares its endianness.
const BYTE_ORDER_MAGIC_LE: u32 = 0x1A2B_3C4D;

/// Interface description option carrying the timestamp resolution.
const OPTION_IF_TSRESOL: u16 = 9;
/// Resolution assumed when an interface declares none: microseconds, per the
/// spec's own default.
const DEFAULT_TSRESOL: u8 = 6;
/// A resolution byte with this bit set counts in powers of two rather than
/// powers of ten.
const TSRESOL_BINARY_FLAG: u8 = 0x80;

/// A block header is a 4-byte type plus a 4-byte total length, and the same
/// total length is repeated as the block's last 4 bytes.
const BLOCK_HEADER_LEN: u32 = 8;
const BLOCK_TRAILER_LEN: u32 = 4;
const MIN_BLOCK_LEN: u32 = BLOCK_HEADER_LEN + BLOCK_TRAILER_LEN;

/// Refuse to allocate for a block claiming to be larger than this. A
/// truncated or corrupt file can name any 32-bit length, and 16 MB is
/// already far past any real packet block.
const MAX_BLOCK_LEN: u32 = 16 * 1024 * 1024;

/// The only link type this module can decode a frame from.
pub const LINKTYPE_ETHERNET: u16 = 1;

const ETHERTYPE_IPV4: u16 = 0x0800;
const ETHERTYPE_VLAN: u16 = 0x8100;
const ETHERNET_HEADER_LEN: usize = 14;
/// An 802.1Q tag inserts four bytes between the source MAC and the real
/// ethertype.
const VLAN_TAG_LEN: usize = 4;

const IP_PROTO_UDP: u8 = 17;
const IPV4_MIN_HEADER_LEN: usize = 20;
const UDP_HEADER_LEN: usize = 8;
/// Set on every fragment but the last one.
const IPV4_FLAG_MORE_FRAGMENTS: u16 = 0x2000;
const IPV4_FRAGMENT_OFFSET_MASK: u16 = 0x1FFF;
/// Fragment offsets are counted in 8-byte units.
const IPV4_FRAGMENT_OFFSET_UNIT: usize = 8;

/// How many partially reassembled datagrams to hold at once. A capture that
/// lost the tail of a fragmented datagram would otherwise leak one entry per
/// occurrence for the length of the file; dropping the oldest incomplete
/// entry loses exactly that already-unusable datagram.
const MAX_PENDING_FRAGMENTS: usize = 256;

/// One reassembled UDP datagram, with the capture timestamp of the packet
/// that completed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Datagram {
    /// Microseconds since the Unix epoch, matching
    /// [`crate::capture::UdpRecord::t`].
    pub time_micros: i64,
    pub src: Ipv4Addr,
    pub dst: Ipv4Addr,
    pub src_port: u16,
    pub dst_port: u16,
    pub payload: Vec<u8>,
}

/// Key identifying the fragments belonging to one IPv4 datagram (RFC 791:
/// source, destination, protocol and identification).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct FragmentKey {
    src: Ipv4Addr,
    dst: Ipv4Addr,
    protocol: u8,
    identification: u16,
}

/// Fragments of one datagram, accumulated until the last one arrives and no
/// hole is left.
#[derive(Debug, Default)]
struct PendingDatagram {
    /// Packet count when this entry was opened, used only to evict the
    /// oldest one once [`MAX_PENDING_FRAGMENTS`] is reached.
    arrival: u64,
    buf: Vec<u8>,
    /// Byte ranges written so far, kept sorted and coalesced so
    /// completeness is a single-range check rather than a per-byte one.
    filled: Vec<(usize, usize)>,
    /// Known once the fragment with the more-fragments flag clear arrives.
    total_len: Option<usize>,
}

impl PendingDatagram {
    fn insert(&mut self, offset: usize, bytes: &[u8], last: bool) {
        let end = offset + bytes.len();
        if self.buf.len() < end {
            self.buf.resize(end, 0);
        }
        self.buf[offset..end].copy_from_slice(bytes);
        self.mark_filled(offset, end);
        if last {
            self.total_len = Some(end);
        }
    }

    fn mark_filled(&mut self, start: usize, end: usize) {
        self.filled.push((start, end));
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

    /// True once the last fragment has arrived and the filled ranges cover
    /// it from zero without a hole.
    fn is_complete(&self) -> bool {
        let Some(total_len) = self.total_len else {
            return false;
        };
        matches!(self.filled.as_slice(), [(0, end)] if *end >= total_len)
    }

    /// The reassembled payload, consuming the entry.
    fn into_payload(self) -> Vec<u8> {
        let total_len = self.total_len.unwrap_or(self.buf.len());
        let mut buf = self.buf;
        buf.truncate(total_len);
        buf
    }
}

/// Timestamp resolution declared by an interface description block.
#[derive(Debug, Clone, Copy)]
struct TimestampResolution {
    value: u8,
    binary: bool,
}

impl TimestampResolution {
    /// Timestamp units per second.
    fn units_per_second(self) -> u64 {
        if self.binary {
            1u64.checked_shl(u32::from(self.value)).unwrap_or(u64::MAX)
        } else {
            10u64.checked_pow(u32::from(self.value)).unwrap_or(u64::MAX)
        }
    }

    fn to_micros(self, ticks: u64) -> i64 {
        let units = self.units_per_second().max(1);
        // Split rather than scaling first: `ticks * 1_000_000` overflows a
        // u64 for any realistic epoch timestamp at nanosecond resolution.
        let seconds = ticks / units;
        let remainder = ticks % units;
        (seconds * 1_000_000 + remainder * 1_000_000 / units) as i64
    }
}

impl Default for TimestampResolution {
    fn default() -> Self {
        Self { value: DEFAULT_TSRESOL, binary: false }
    }
}

/// One capture interface, as described by an interface description block.
#[derive(Debug, Clone, Copy)]
struct Interface {
    link_type: u16,
    resolution: TimestampResolution,
}

/// What a capture turned out to contain, for the caller to report.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PcapngStats {
    /// Packet blocks walked, whatever they held.
    pub packets_seen: u64,
    /// Packets that were not UDP over IPv4 over Ethernet, or were too short
    /// to decode. Counted rather than logged so the caller can state
    /// coverage instead of implying the capture held only Pro DJ Link.
    pub packets_skipped: u64,
    /// Packets recorded on an interface whose link type this module cannot
    /// decode. Also counted in [`Self::packets_skipped`]; broken out because
    /// a whole-file mismatch means something quite different from stray
    /// non-IPv4 traffic.
    pub packets_unsupported_link: u64,
    /// Datagrams still missing a fragment when the file ended.
    pub incomplete_datagrams: u64,
}

/// Reads a pcapng file block by block, yielding the UDP datagrams it holds.
///
/// Takes a [`Read`] rather than a path so tests can drive it from bytes in
/// memory; wrap a file in a [`std::io::BufReader`], since this reads in
/// pieces as small as four bytes.
pub struct PcapngReader<R: Read> {
    reader: R,
    /// Sections may declare either endianness, and one file may hold
    /// several sections.
    big_endian: bool,
    /// Interfaces in the order they were described; packet blocks reference
    /// them by that index.
    interfaces: Vec<Interface>,
    pending: HashMap<FragmentKey, PendingDatagram>,
    /// Decoded from the packet currently being processed but not yet
    /// returned. A packet carries at most one datagram; holding it here
    /// keeps [`Self::next_datagram`] a plain loop.
    ready: Option<Datagram>,
    stats: PcapngStats,
}

impl<R: Read> PcapngReader<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            big_endian: false,
            interfaces: Vec::new(),
            pending: HashMap::new(),
            ready: None,
            stats: PcapngStats::default(),
        }
    }

    /// Counts so far; final once [`Self::next_datagram`] has returned
    /// `None`.
    pub fn stats(&self) -> PcapngStats {
        PcapngStats { incomplete_datagrams: self.pending.len() as u64, ..self.stats }
    }

    /// Returns the next UDP datagram, or `None` at end of file.
    ///
    /// An `Err` means the file could not be walked any further (a truncated
    /// or corrupt block). Packets that merely fail to decode are skipped and
    /// counted in [`PcapngStats`], since a real booth capture carries plenty
    /// of traffic that is none of this module's business.
    pub fn next_datagram(&mut self) -> io::Result<Option<Datagram>> {
        loop {
            if let Some(datagram) = self.ready.take() {
                return Ok(Some(datagram));
            }
            let Some((block_type, body)) = self.read_block()? else {
                return Ok(None);
            };
            match block_type {
                BLOCK_INTERFACE_DESCRIPTION => self.read_interface(&body),
                BLOCK_ENHANCED_PACKET => self.read_packet(&body),
                _ => {}
            }
        }
    }

    /// Reads one block, returning its type and body (everything between the
    /// 8-byte header and the trailing length).
    fn read_block(&mut self) -> io::Result<Option<(u32, Vec<u8>)>> {
        let mut header = [0u8; BLOCK_HEADER_LEN as usize];
        if !read_exact_or_eof(&mut self.reader, &mut header)? {
            return Ok(None);
        }
        // A section header declares the endianness of everything after it,
        // including its own length field, so its type has to be recognised
        // from the raw bytes before any endianness is known. The magic is a
        // palindrome across byte order, which is what makes that work.
        if u32::from_le_bytes([header[0], header[1], header[2], header[3]]) == BLOCK_SECTION_HEADER {
            return self.read_section_header(&header).map(Some);
        }
        let block_type = self.u32_at(&header, 0);
        let block_len = self.u32_at(&header, 4);
        let body = self.read_block_body(block_len)?;
        Ok(Some((block_type, body)))
    }

    /// Consumes a section header, adopting the endianness it declares and
    /// resetting the interface table (ids restart at zero in each section).
    fn read_section_header(&mut self, header: &[u8; BLOCK_HEADER_LEN as usize]) -> io::Result<(u32, Vec<u8>)> {
        const MAGIC_LEN: u32 = 4;
        let mut magic = [0u8; MAGIC_LEN as usize];
        self.reader.read_exact(&mut magic)?;
        self.big_endian = u32::from_le_bytes(magic) != BYTE_ORDER_MAGIC_LE;
        let block_len = self.u32_at(header, 4);
        let remaining = block_len
            .checked_sub(MIN_BLOCK_LEN + MAGIC_LEN)
            .ok_or_else(|| invalid_data(format!("section header block length {block_len} is too small")))?;
        if remaining > MAX_BLOCK_LEN {
            return Err(invalid_data(format!("section header block length {block_len} exceeds the {MAX_BLOCK_LEN}-byte limit")));
        }
        let mut body = magic.to_vec();
        body.extend(self.read_exact_vec(remaining)?);
        self.skip_trailer()?;
        self.interfaces.clear();
        Ok((BLOCK_SECTION_HEADER, body))
    }

    fn read_block_body(&mut self, block_len: u32) -> io::Result<Vec<u8>> {
        if block_len < MIN_BLOCK_LEN {
            return Err(invalid_data(format!("block length {block_len} is shorter than an empty block")));
        }
        if block_len > MAX_BLOCK_LEN {
            return Err(invalid_data(format!("block length {block_len} exceeds the {MAX_BLOCK_LEN}-byte limit")));
        }
        let body = self.read_exact_vec(block_len - MIN_BLOCK_LEN)?;
        self.skip_trailer()?;
        Ok(body)
    }

    fn skip_trailer(&mut self) -> io::Result<()> {
        let mut trailer = [0u8; BLOCK_TRAILER_LEN as usize];
        self.reader.read_exact(&mut trailer)
    }

    fn read_exact_vec(&mut self, len: u32) -> io::Result<Vec<u8>> {
        let mut buf = vec![0u8; len as usize];
        self.reader.read_exact(&mut buf)?;
        Ok(buf)
    }

    fn u16_at(&self, bytes: &[u8], offset: usize) -> u16 {
        let raw = [bytes[offset], bytes[offset + 1]];
        if self.big_endian {
            u16::from_be_bytes(raw)
        } else {
            u16::from_le_bytes(raw)
        }
    }

    fn u32_at(&self, bytes: &[u8], offset: usize) -> u32 {
        let raw = [bytes[offset], bytes[offset + 1], bytes[offset + 2], bytes[offset + 3]];
        if self.big_endian {
            u32::from_be_bytes(raw)
        } else {
            u32::from_le_bytes(raw)
        }
    }

    /// Interface description block: link type, then options, of which only
    /// the timestamp resolution matters here. An interface this module
    /// cannot decode frames from is still recorded, so packet blocks keep
    /// resolving their interface id to the right entry.
    fn read_interface(&mut self, body: &[u8]) {
        // Link type (2) + reserved (2) + snaplen (4), then options.
        const OPTIONS_OFFSET: usize = 8;
        if body.len() < OPTIONS_OFFSET {
            self.interfaces.push(Interface { link_type: LINKTYPE_ETHERNET, resolution: TimestampResolution::default() });
            return;
        }
        self.interfaces.push(Interface {
            link_type: self.u16_at(body, 0),
            resolution: self.read_tsresol_option(&body[OPTIONS_OFFSET..]),
        });
    }

    fn read_tsresol_option(&self, mut options: &[u8]) -> TimestampResolution {
        const OPTION_HEADER_LEN: usize = 4;
        while options.len() >= OPTION_HEADER_LEN {
            let code = self.u16_at(options, 0);
            let len = usize::from(self.u16_at(options, 2));
            let value_end = OPTION_HEADER_LEN + len;
            if value_end > options.len() {
                break;
            }
            if code == OPTION_IF_TSRESOL && len >= 1 {
                let raw = options[OPTION_HEADER_LEN];
                return TimestampResolution { value: raw & !TSRESOL_BINARY_FLAG, binary: raw & TSRESOL_BINARY_FLAG != 0 };
            }
            // Option values are padded to a 4-byte boundary.
            let padded = value_end.div_ceil(4) * 4;
            if padded >= options.len() {
                break;
            }
            options = &options[padded..];
        }
        TimestampResolution::default()
    }

    /// Enhanced packet block: interface id, split timestamp, captured and
    /// original lengths, then the frame itself.
    fn read_packet(&mut self, body: &[u8]) {
        const FRAME_OFFSET: usize = 20;
        if body.len() < FRAME_OFFSET {
            self.stats.packets_skipped += 1;
            return;
        }
        self.stats.packets_seen += 1;
        let interface_id = self.u32_at(body, 0) as usize;
        let ts_high = u64::from(self.u32_at(body, 4));
        let ts_low = u64::from(self.u32_at(body, 8));
        let captured_len = self.u32_at(body, 12) as usize;
        let frame_end = FRAME_OFFSET + captured_len;
        if frame_end > body.len() {
            self.stats.packets_skipped += 1;
            return;
        }
        let interface = self
            .interfaces
            .get(interface_id)
            .copied()
            .unwrap_or(Interface { link_type: LINKTYPE_ETHERNET, resolution: TimestampResolution::default() });
        if interface.link_type != LINKTYPE_ETHERNET {
            self.stats.packets_skipped += 1;
            self.stats.packets_unsupported_link += 1;
            return;
        }
        let time_micros = interface.resolution.to_micros((ts_high << 32) | ts_low);
        match self.decode_frame(time_micros, &body[FRAME_OFFSET..frame_end]) {
            Some(datagram) => self.ready = Some(datagram),
            None => self.stats.packets_skipped += 1,
        }
    }

    /// Ethernet, then IPv4, then UDP, reassembling fragments on the way.
    /// `None` for anything else, and for a fragment that does not complete a
    /// datagram.
    fn decode_frame(&mut self, time_micros: i64, frame: &[u8]) -> Option<Datagram> {
        let packet = ipv4_payload_of(frame)?;
        if packet.len() < IPV4_MIN_HEADER_LEN {
            return None;
        }
        let header_len = usize::from(packet[0] & 0x0F) * 4;
        let total_len = usize::from(u16::from_be_bytes([packet[2], packet[3]]));
        let identification = u16::from_be_bytes([packet[4], packet[5]]);
        let flags_and_offset = u16::from_be_bytes([packet[6], packet[7]]);
        // Byte 8 is the TTL; the protocol number is byte 9.
        let protocol = packet[9];
        if protocol != IP_PROTO_UDP || !(IPV4_MIN_HEADER_LEN..=packet.len()).contains(&header_len) {
            return None;
        }
        let src = Ipv4Addr::new(packet[12], packet[13], packet[14], packet[15]);
        let dst = Ipv4Addr::new(packet[16], packet[17], packet[18], packet[19]);
        // Ethernet pads short frames, so bytes past the IP total length are
        // not payload; a capture cut short by snaplen holds fewer bytes than
        // that length claims. Taking the smaller of the two covers both.
        let payload_end = total_len.max(header_len).min(packet.len());
        let payload = &packet[header_len..payload_end];

        let more_fragments = flags_and_offset & IPV4_FLAG_MORE_FRAGMENTS != 0;
        let fragment_offset = usize::from(flags_and_offset & IPV4_FRAGMENT_OFFSET_MASK) * IPV4_FRAGMENT_OFFSET_UNIT;
        let transport = if more_fragments || fragment_offset > 0 {
            let key = FragmentKey { src, dst, protocol, identification };
            self.reassemble(key, fragment_offset, payload, !more_fragments)?
        } else {
            payload.to_vec()
        };

        if transport.len() < UDP_HEADER_LEN {
            return None;
        }
        let src_port = u16::from_be_bytes([transport[0], transport[1]]);
        let dst_port = u16::from_be_bytes([transport[2], transport[3]]);
        // The UDP length field covers its own header too, and a capture cut
        // short by snaplen can hold less than it claims.
        let udp_len = usize::from(u16::from_be_bytes([transport[4], transport[5]]))
            .max(UDP_HEADER_LEN)
            .min(transport.len());
        Some(Datagram { time_micros, src, dst, src_port, dst_port, payload: transport[UDP_HEADER_LEN..udp_len].to_vec() })
    }

    /// Adds one fragment, returning the whole datagram once the last
    /// fragment has arrived and nothing is missing.
    fn reassemble(&mut self, key: FragmentKey, offset: usize, payload: &[u8], last: bool) -> Option<Vec<u8>> {
        if self.pending.len() >= MAX_PENDING_FRAGMENTS && !self.pending.contains_key(&key) {
            self.evict_oldest_pending();
        }
        let arrival = self.stats.packets_seen;
        let entry = self.pending.entry(key).or_insert_with(|| PendingDatagram { arrival, ..Default::default() });
        entry.insert(offset, payload, last);
        entry.is_complete().then(|| self.pending.remove(&key))?.map(PendingDatagram::into_payload)
    }

    fn evict_oldest_pending(&mut self) {
        let oldest = self.pending.iter().min_by_key(|(_, entry)| entry.arrival).map(|(key, _)| *key);
        if let Some(key) = oldest {
            self.pending.remove(&key);
        }
    }
}

/// The IPv4 packet inside an Ethernet frame, unwrapping one 802.1Q tag.
fn ipv4_payload_of(frame: &[u8]) -> Option<&[u8]> {
    if frame.len() < ETHERNET_HEADER_LEN {
        return None;
    }
    match u16::from_be_bytes([frame[12], frame[13]]) {
        ETHERTYPE_IPV4 => Some(&frame[ETHERNET_HEADER_LEN..]),
        ETHERTYPE_VLAN => {
            let tagged = ETHERNET_HEADER_LEN + VLAN_TAG_LEN;
            if frame.len() < tagged {
                return None;
            }
            let inner = u16::from_be_bytes([frame[tagged - 2], frame[tagged - 1]]);
            (inner == ETHERTYPE_IPV4).then(|| &frame[tagged..])
        }
        _ => None,
    }
}

/// Fills `buf`, reporting a clean end of file (`Ok(false)`) only when
/// nothing at all was left — a partial read means a truncated file.
fn read_exact_or_eof<R: Read>(reader: &mut R, buf: &mut [u8]) -> io::Result<bool> {
    let mut filled = 0;
    while filled < buf.len() {
        match reader.read(&mut buf[filled..])? {
            0 if filled == 0 => return Ok(false),
            0 => return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "pcapng file ends mid-block")),
            read => filled += read,
        }
    }
    Ok(true)
}

fn invalid_data(message: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
