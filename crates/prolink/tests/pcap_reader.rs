//! Pins [`prolink::pcap::PcapngReader`] against hand-built pcapng files.
//!
//! Every case here is built byte by byte rather than read from a fixture:
//! a real booth capture holds a DJ's library metadata and is never
//! committed (see `.gitignore` and `HARDWARE-CAPTURE-PLAN.md`), and the
//! structural facts worth pinning — section endianness, timestamp
//! resolution, IPv4 fragment reassembly — are all reproducible in a few
//! dozen bytes.

use std::net::Ipv4Addr;

use prolink::pcap::{Datagram, PcapngReader};

const BLOCK_SECTION_HEADER: u32 = 0x0A0D_0D0A;
const BLOCK_INTERFACE_DESCRIPTION: u32 = 0x0000_0001;
const BLOCK_ENHANCED_PACKET: u32 = 0x0000_0006;
const BYTE_ORDER_MAGIC: u32 = 0x1A2B_3C4D;
const LINKTYPE_ETHERNET: u16 = 1;
const LINKTYPE_RAW: u16 = 101;
const ETHERTYPE_IPV4: u16 = 0x0800;
const ETHERTYPE_ARP: u16 = 0x0806;
const ETHERTYPE_VLAN: u16 = 0x8100;
const IP_PROTO_UDP: u8 = 17;

/// Builds pcapng blocks in one endianness, so a test reads as a list of the
/// packets it means to describe rather than as byte arithmetic.
struct PcapngBuilder {
    bytes: Vec<u8>,
    big_endian: bool,
}

impl PcapngBuilder {
    fn new(big_endian: bool) -> Self {
        let mut builder = Self { bytes: Vec::new(), big_endian };
        builder.push_section_header();
        builder
    }

    fn push_section_header(&mut self) {
        let mut body = self.u32(BYTE_ORDER_MAGIC);
        body.extend(self.u16(1)); // major version
        body.extend(self.u16(0)); // minor version
        body.extend(vec![0xFF; 8]); // section length: not specified
        self.push_block(BLOCK_SECTION_HEADER, &body);
    }

    /// Interface description block, optionally declaring a timestamp
    /// resolution other than the default microseconds.
    fn push_interface(&mut self, link_type: u16, tsresol: Option<u8>) {
        let mut body = self.u16(link_type);
        body.extend(self.u16(0)); // reserved
        body.extend(self.u32(0xFFFF)); // snaplen
        if let Some(tsresol) = tsresol {
            body.extend(self.u16(9)); // if_tsresol
            body.extend(self.u16(1)); // length
            body.extend([tsresol, 0, 0, 0]); // value, padded to 4 bytes
            body.extend(self.u16(0)); // opt_endofopt
            body.extend(self.u16(0));
        }
        self.push_block(BLOCK_INTERFACE_DESCRIPTION, &body);
    }

    fn push_packet(&mut self, timestamp_ticks: u64, frame: &[u8]) {
        let mut body = self.u32(0); // interface id
        body.extend(self.u32((timestamp_ticks >> 32) as u32));
        body.extend(self.u32(timestamp_ticks as u32));
        body.extend(self.u32(frame.len() as u32)); // captured length
        body.extend(self.u32(frame.len() as u32)); // original length
        body.extend(frame);
        body.extend(vec![0; padding_for(frame.len())]);
        self.push_block(BLOCK_ENHANCED_PACKET, &body);
    }

    fn push_block(&mut self, block_type: u32, body: &[u8]) {
        let total_len = 12 + body.len() as u32;
        self.bytes.extend(self.u32(block_type));
        self.bytes.extend(self.u32(total_len));
        self.bytes.extend(body);
        self.bytes.extend(self.u32(total_len));
    }

    fn u16(&self, value: u16) -> Vec<u8> {
        if self.big_endian {
            value.to_be_bytes().to_vec()
        } else {
            value.to_le_bytes().to_vec()
        }
    }

    fn u32(&self, value: u32) -> Vec<u8> {
        if self.big_endian {
            value.to_be_bytes().to_vec()
        } else {
            value.to_le_bytes().to_vec()
        }
    }

    fn datagrams(&self) -> Vec<Datagram> {
        let mut reader = PcapngReader::new(self.bytes.as_slice());
        let mut datagrams = Vec::new();
        while let Some(datagram) = reader.next_datagram().expect("the built file should walk cleanly") {
            datagrams.push(datagram);
        }
        datagrams
    }
}

/// Packet block bodies are padded to a 4-byte boundary.
fn padding_for(len: usize) -> usize {
    (4 - len % 4) % 4
}

/// Ethernet header with the given ethertype; MAC addresses are arbitrary.
fn ethernet(ethertype: u16) -> Vec<u8> {
    let mut frame = vec![0x00, 0xE0, 0x4C, 0x68, 0x01, 0x4C, 0xC8, 0x3D, 0xFC, 0x14, 0x32, 0x98];
    frame.extend(ethertype.to_be_bytes());
    frame
}

struct Ipv4Options {
    identification: u16,
    /// Offset in bytes; must be a multiple of eight, as on the wire.
    fragment_offset: usize,
    more_fragments: bool,
}

impl Default for Ipv4Options {
    fn default() -> Self {
        Self { identification: 0x4601, fragment_offset: 0, more_fragments: false }
    }
}

/// A complete Ethernet/IPv4 frame carrying `payload` as the IP payload.
fn ipv4_frame(src: Ipv4Addr, dst: Ipv4Addr, payload: &[u8], options: Ipv4Options) -> Vec<u8> {
    let mut frame = ethernet(ETHERTYPE_IPV4);
    let total_len = (20 + payload.len()) as u16;
    let flags_and_offset = (options.fragment_offset as u16 / 8) | if options.more_fragments { 0x2000 } else { 0x4000 };
    frame.extend([0x45, 0x00]);
    frame.extend(total_len.to_be_bytes());
    frame.extend(options.identification.to_be_bytes());
    frame.extend(flags_and_offset.to_be_bytes());
    frame.extend([0x40, IP_PROTO_UDP, 0x00, 0x00]); // TTL, protocol, checksum
    frame.extend(src.octets());
    frame.extend(dst.octets());
    frame.extend(payload);
    frame
}

/// A UDP header followed by `payload`.
fn udp(src_port: u16, dst_port: u16, payload: &[u8]) -> Vec<u8> {
    let mut datagram = Vec::new();
    datagram.extend(src_port.to_be_bytes());
    datagram.extend(dst_port.to_be_bytes());
    datagram.extend(((8 + payload.len()) as u16).to_be_bytes());
    datagram.extend([0x00, 0x00]); // checksum
    datagram.extend(payload);
    datagram
}

const PLAYER: Ipv4Addr = Ipv4Addr::new(169, 254, 7, 50);
const LISTENER: Ipv4Addr = Ipv4Addr::new(169, 254, 194, 207);

#[test]
fn reads_a_single_udp_datagram() {
    let mut builder = PcapngBuilder::new(false);
    builder.push_interface(LINKTYPE_ETHERNET, None);
    let frame = ipv4_frame(PLAYER, LISTENER, &udp(50002, 50002, b"status"), Ipv4Options::default());
    builder.push_packet(1_700_000_000_000_000, &frame);

    let datagrams = builder.datagrams();
    assert_eq!(datagrams.len(), 1);
    assert_eq!(
        datagrams[0],
        Datagram {
            time_micros: 1_700_000_000_000_000,
            src: PLAYER,
            dst: LISTENER,
            src_port: 50002,
            dst_port: 50002,
            payload: b"status".to_vec(),
        }
    );
}

/// A big-endian section declares itself through the byte-order magic alone,
/// and every length field after it — including its own — follows.
#[test]
fn reads_a_big_endian_section() {
    let mut builder = PcapngBuilder::new(true);
    builder.push_interface(LINKTYPE_ETHERNET, None);
    let frame = ipv4_frame(PLAYER, LISTENER, &udp(50001, 50001, b"beat"), Ipv4Options::default());
    builder.push_packet(42_000_000, &frame);

    let datagrams = builder.datagrams();
    assert_eq!(datagrams.len(), 1);
    assert_eq!(datagrams[0].payload, b"beat".to_vec());
    assert_eq!(datagrams[0].time_micros, 42_000_000);
}

/// Wireshark writes nanosecond captures by declaring `if_tsresol` 9; read as
/// the default microseconds, every timestamp would be a thousand times too
/// large.
#[test]
fn honours_the_declared_timestamp_resolution() {
    let mut builder = PcapngBuilder::new(false);
    builder.push_interface(LINKTYPE_ETHERNET, Some(9));
    let frame = ipv4_frame(PLAYER, LISTENER, &udp(50000, 50000, b"keepalive"), Ipv4Options::default());
    builder.push_packet(1_700_000_000_123_456_789, &frame);

    assert_eq!(builder.datagrams()[0].time_micros, 1_700_000_000_123_456);
}

/// An 8 KB NFS read crosses a 1500-byte-MTU booth network as fragments, and
/// none of them alone carries a usable UDP header.
#[test]
fn reassembles_a_fragmented_datagram() {
    let mut builder = PcapngBuilder::new(false);
    builder.push_interface(LINKTYPE_ETHERNET, None);

    let analysis: Vec<u8> = (0..3_000u32).map(|byte| byte as u8).collect();
    let datagram = udp(2049, 60032, &analysis);
    // Fragment offsets count in 8-byte units, so every fragment but the last
    // has to be a multiple of eight bytes long.
    let (first, rest) = datagram.split_at(1_480);
    let (second, third) = rest.split_at(1_480);
    let identification = 0x1234;
    for (offset, payload, more) in
        [(0, first, true), (1_480, second, true), (2_960, third, false)]
    {
        let frame = ipv4_frame(
            PLAYER,
            LISTENER,
            payload,
            Ipv4Options { identification, fragment_offset: offset, more_fragments: more },
        );
        builder.push_packet(1_000, &frame);
    }

    let datagrams = builder.datagrams();
    assert_eq!(datagrams.len(), 1, "the three fragments should yield one datagram");
    assert_eq!(datagrams[0].src_port, 2049);
    assert_eq!(datagrams[0].payload, analysis);
}

/// Fragments do not have to arrive in order, and a datagram is only complete
/// once the gap between them is filled.
#[test]
fn reassembles_fragments_arriving_out_of_order() {
    let mut builder = PcapngBuilder::new(false);
    builder.push_interface(LINKTYPE_ETHERNET, None);

    let payload: Vec<u8> = (0..2_000u32).map(|byte| (byte % 251) as u8).collect();
    let datagram = udp(2049, 60032, &payload);
    let (first, second) = datagram.split_at(1_480);
    for (offset, part, more) in [(1_480, second, false), (0, first, true)] {
        let frame = ipv4_frame(
            PLAYER,
            LISTENER,
            part,
            Ipv4Options { identification: 0x9999, fragment_offset: offset, more_fragments: more },
        );
        builder.push_packet(2_000, &frame);
    }

    let datagrams = builder.datagrams();
    assert_eq!(datagrams.len(), 1);
    assert_eq!(datagrams[0].payload, payload);
}

/// A fragment whose neighbours never arrive must not surface as a truncated
/// datagram; it is reported as incomplete instead.
#[test]
fn drops_a_datagram_whose_fragments_never_complete() {
    let mut builder = PcapngBuilder::new(false);
    builder.push_interface(LINKTYPE_ETHERNET, None);
    let frame = ipv4_frame(
        PLAYER,
        LISTENER,
        &udp(2049, 60032, &vec![0xAB; 1_400]),
        Ipv4Options { identification: 0x4242, fragment_offset: 0, more_fragments: true },
    );
    builder.push_packet(3_000, &frame);

    let mut reader = PcapngReader::new(builder.bytes.as_slice());
    assert_eq!(reader.next_datagram().expect("walks cleanly"), None);
    assert_eq!(reader.stats().incomplete_datagrams, 1);
}

/// A booth switch carries far more than Pro DJ Link; none of it may abort
/// the walk.
#[test]
fn skips_traffic_it_cannot_decode() {
    let mut builder = PcapngBuilder::new(false);
    builder.push_interface(LINKTYPE_ETHERNET, None);
    builder.push_packet(1, &ethernet(ETHERTYPE_ARP));
    let mut icmp = ipv4_frame(PLAYER, LISTENER, &[0x08, 0x00, 0x00, 0x00], Ipv4Options::default());
    icmp[23] = 1; // rewrite the IP protocol field to ICMP
    builder.push_packet(2, &icmp);
    let udp_frame = ipv4_frame(PLAYER, LISTENER, &udp(50002, 50002, b"kept"), Ipv4Options::default());
    builder.push_packet(3, &udp_frame);

    let mut reader = PcapngReader::new(builder.bytes.as_slice());
    let mut payloads = Vec::new();
    while let Some(datagram) = reader.next_datagram().expect("walks cleanly") {
        payloads.push(datagram.payload);
    }
    assert_eq!(payloads, vec![b"kept".to_vec()]);
    assert_eq!(reader.stats().packets_seen, 3);
    assert_eq!(reader.stats().packets_skipped, 2);
}

/// A capture taken on a tagged port wraps the same traffic in an 802.1Q tag.
#[test]
fn reads_through_a_vlan_tag() {
    let mut builder = PcapngBuilder::new(false);
    builder.push_interface(LINKTYPE_ETHERNET, None);

    let inner = ipv4_frame(PLAYER, LISTENER, &udp(50002, 50002, b"tagged"), Ipv4Options::default());
    let mut frame = ethernet(ETHERTYPE_VLAN);
    frame.extend([0x00, 0x64]); // priority and VLAN id 100
    frame.extend(ETHERTYPE_IPV4.to_be_bytes());
    frame.extend(&inner[14..]);
    builder.push_packet(4, &frame);

    assert_eq!(builder.datagrams()[0].payload, b"tagged".to_vec());
}

/// A capture from a non-Ethernet interface is reported rather than silently
/// yielding nothing, since that is a whole-file mismatch and not stray
/// traffic.
#[test]
fn reports_an_unsupported_link_type() {
    let mut builder = PcapngBuilder::new(false);
    builder.push_interface(LINKTYPE_RAW, None);
    builder.push_packet(5, &[0x45, 0x00, 0x00, 0x1C]);

    let mut reader = PcapngReader::new(builder.bytes.as_slice());
    assert_eq!(reader.next_datagram().expect("walks cleanly"), None);
    assert_eq!(reader.stats().packets_unsupported_link, 1);
}

/// A file cut short mid-block is an error, not a silent end of capture: an
/// importer that treated it as EOF would quietly drop the rest of a session.
#[test]
fn rejects_a_truncated_file() {
    let mut builder = PcapngBuilder::new(false);
    builder.push_interface(LINKTYPE_ETHERNET, None);
    let frame = ipv4_frame(PLAYER, LISTENER, &udp(50002, 50002, b"cut"), Ipv4Options::default());
    builder.push_packet(6, &frame);
    builder.bytes.truncate(builder.bytes.len() - 8);

    let mut reader = PcapngReader::new(builder.bytes.as_slice());
    assert!(reader.next_datagram().is_err(), "a block cut short must not read as end of file");
}
