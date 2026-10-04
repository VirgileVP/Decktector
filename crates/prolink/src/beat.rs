//! Beat packets and CDJ-3000 precise-position packets — both on
//! [`crate::BEAT_PORT`], distinguished by the packet-type byte.
//!
//! A beat packet is sent once per actual beat (so its rate follows the
//! track's tempo, unlike status packets' fixed ~5 Hz) and carries countdowns
//! to upcoming beat/bar boundaries — useful for tight sync, though
//! `prolink-engine`'s playhead estimate for this phase only needs its pitch
//! and beat-in-bar fields, already available from status packets too.
//!
//! A precise-position packet is CDJ-3000-only and is the sole source of an
//! actual absolute playhead position on this hardware generation — every
//! other player only ever reports a beat counter (see [`crate::status`]).
//!
//! Byte offsets for the beat packet come from Deep Symmetry's analysis
//! (<https://djl-analysis.deepsymmetry.org/djl-analysis/beats.html>). That
//! same page describes the precise-position packet as type `0x02` with a
//! variable length driven by a `lenr` field; this implementation instead
//! follows decktector's own pre-Rust engine, whose comments record its
//! packet shape (type `0x0b`, fixed 60 bytes, position at `0x28`) as
//! byte-for-byte verified against real CDJ-3000 hardware. That's stronger
//! provenance than a doc page for this one packet, but the two sources
//! disagree — confirm against a real capture before relying on this in a
//! venue, and reconcile the discrepancy here if it turns out wrong.

use std::net::IpAddr;
use std::sync::Arc;

use tokio::sync::broadcast;

use crate::{bind_udp_socket, decode_pitch, BEAT_PORT, PROLINK_HEADER};

const EVENT_BUFFER: usize = 256;

const BEAT_PACKET_TYPE: u8 = 0x28;
const BEAT_PACKET_LEN: usize = 0x60;
const BEAT_DEVICE_NUMBER_OFFSET: usize = 0x21;
const BEAT_PITCH_OFFSET: usize = 0x54;
const BEAT_BPM_OFFSET: usize = 0x5a;
const BEAT_IN_BAR_OFFSET: usize = 0x5c;

/// See this module's doc comment: sourced from decktector's pre-Rust
/// engine's hardware-verified comments, not the doc site (which names a
/// different type byte and a variable length for this packet).
const PRECISE_POSITION_PACKET_TYPE: u8 = 0x0b;
const PRECISE_POSITION_PACKET_LEN: usize = 0x3c;
const PRECISE_POSITION_DEVICE_NUMBER_OFFSET: usize = 0x21;
const PRECISE_POSITION_MS_OFFSET: usize = 0x28;

/// A regular beat packet: fired once per beat, carrying countdowns to
/// upcoming beat/bar boundaries plus the same pitch/BPM/beat-in-bar fields a
/// status packet carries.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BeatPacket {
    pub device_number: u8,
    pub pitch_percent: f64,
    /// BPM of the loaded track at 0% pitch; `None` when the player reports
    /// the "no track" sentinel.
    pub bpm: Option<f64>,
    /// Position within the bar, 1-4.
    pub beat_in_bar: u8,
}

/// A CDJ-3000 precise-position packet: the actual playhead position, in
/// milliseconds from the start of the track.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PrecisePosition {
    pub device_number: u8,
    pub position_ms: u32,
}

/// Either message [`crate::BEAT_PORT`] carries.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BeatMessage {
    Beat(BeatPacket),
    PrecisePosition(PrecisePosition),
}

/// Parses a beat-port packet's payload; `None` if it matches neither known
/// shape.
pub fn parse_beat_message(data: &[u8]) -> Option<BeatMessage> {
    if !data.starts_with(&PROLINK_HEADER) {
        return None;
    }
    match data[PROLINK_HEADER.len()] {
        BEAT_PACKET_TYPE if data.len() >= BEAT_PACKET_LEN => {
            let bpm_raw = read_u16(data, BEAT_BPM_OFFSET);
            Some(BeatMessage::Beat(BeatPacket {
                device_number: data[BEAT_DEVICE_NUMBER_OFFSET],
                pitch_percent: decode_pitch(read_u32(data, BEAT_PITCH_OFFSET)),
                bpm: (bpm_raw != 0xffff).then(|| f64::from(bpm_raw) / 100.0),
                beat_in_bar: data[BEAT_IN_BAR_OFFSET],
            }))
        }
        PRECISE_POSITION_PACKET_TYPE if data.len() >= PRECISE_POSITION_PACKET_LEN => {
            Some(BeatMessage::PrecisePosition(PrecisePosition {
                device_number: data[PRECISE_POSITION_DEVICE_NUMBER_OFFSET],
                position_ms: read_u32(data, PRECISE_POSITION_MS_OFFSET),
            }))
        }
        _ => None,
    }
}

/// Builds a well-formed beat packet. Used by tests and by the `cdj-sim` dev
/// tool.
pub fn build_beat_packet(
    device_number: u8,
    pitch_percent: f64,
    bpm: Option<f64>,
    beat_in_bar: u8,
) -> Vec<u8> {
    let mut packet = vec![0u8; BEAT_PACKET_LEN];
    packet[..PROLINK_HEADER.len()].copy_from_slice(&PROLINK_HEADER);
    packet[PROLINK_HEADER.len()] = BEAT_PACKET_TYPE;
    packet[BEAT_DEVICE_NUMBER_OFFSET] = device_number;

    let pitch_raw = ((pitch_percent / 100.0 + 1.0) * f64::from(0x0010_0000u32)) as u32;
    write_u32(&mut packet, BEAT_PITCH_OFFSET, pitch_raw);
    write_u16(
        &mut packet,
        BEAT_BPM_OFFSET,
        bpm.map_or(0xffff, |bpm| (bpm * 100.0) as u16),
    );
    packet[BEAT_IN_BAR_OFFSET] = beat_in_bar;

    packet
}

/// Builds a well-formed precise-position packet. Used by tests and by the
/// `cdj-sim` dev tool.
pub fn build_precise_position(device_number: u8, position_ms: u32) -> Vec<u8> {
    let mut packet = vec![0u8; PRECISE_POSITION_PACKET_LEN];
    packet[..PROLINK_HEADER.len()].copy_from_slice(&PROLINK_HEADER);
    packet[PROLINK_HEADER.len()] = PRECISE_POSITION_PACKET_TYPE;
    packet[PRECISE_POSITION_DEVICE_NUMBER_OFFSET] = device_number;
    write_u32(&mut packet, PRECISE_POSITION_MS_OFFSET, position_ms);
    packet
}

fn read_u16(data: &[u8], offset: usize) -> u16 {
    u16::from_be_bytes([data[offset], data[offset + 1]])
}

fn read_u32(data: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
}

fn write_u16(data: &mut [u8], offset: usize, value: u16) {
    data[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
}

fn write_u32(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
}

/// Listens for beat-port packets and republishes each one as it arrives —
/// same shape as [`crate::status::StatusWatcher`], for the same reason (the
/// caller already owns a snapshot layer, so every parsed message is simply
/// forwarded).
pub struct BeatWatcher {
    messages_tx: broadcast::Sender<BeatMessage>,
}

impl BeatWatcher {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            messages_tx: broadcast::channel(EVENT_BUFFER).0,
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<BeatMessage> {
        self.messages_tx.subscribe()
    }

    /// Runs until the socket errors out or the process exits. A bind
    /// failure is logged and this watcher simply never reports anything,
    /// rather than taking the engine down over one socket.
    pub async fn run(self: Arc<Self>) {
        let socket = match bind_udp_socket(BEAT_PORT) {
            Ok(socket) => socket,
            Err(error) => {
                tracing::error!(%error, "failed to bind the Pro DJ Link beat socket");
                return;
            }
        };

        let mut buf = [0u8; 512];
        loop {
            let (len, _source): (_, IpAddr) = match socket.recv_from(&mut buf).await {
                Ok((len, addr)) => (len, addr.ip()),
                Err(error) => {
                    tracing::warn!(%error, "failed to read from the beat socket");
                    continue;
                }
            };
            if let Some(message) = parse_beat_message(&buf[..len]) {
                // No subscribers yet (engine still starting) is not an error.
                let _ = self.messages_tx.send(message);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_beat_packet() {
        let bytes = build_beat_packet(3, -1.5, Some(174.0), 2);
        let message = parse_beat_message(&bytes).expect("should parse");

        match message {
            BeatMessage::Beat(beat) => {
                assert_eq!(beat.device_number, 3);
                assert!((beat.pitch_percent - -1.5).abs() < 0.01);
                assert_eq!(beat.bpm, Some(174.0));
                assert_eq!(beat.beat_in_bar, 2);
            }
            BeatMessage::PrecisePosition(_) => panic!("expected a beat packet"),
        }
    }

    #[test]
    fn parses_a_precise_position_packet() {
        let bytes = build_precise_position(4, 123_456);
        let message = parse_beat_message(&bytes).expect("should parse");

        match message {
            BeatMessage::PrecisePosition(position) => {
                assert_eq!(position.device_number, 4);
                assert_eq!(position.position_ms, 123_456);
            }
            BeatMessage::Beat(_) => panic!("expected a precise-position packet"),
        }
    }

    #[test]
    fn a_beat_with_no_bpm_reports_none() {
        let bytes = build_beat_packet(1, 0.0, None, 1);
        let message = parse_beat_message(&bytes).expect("should parse");
        assert!(matches!(
            message,
            BeatMessage::Beat(BeatPacket { bpm: None, .. })
        ));
    }

    #[test]
    fn rejects_a_foreign_header() {
        let mut bytes = build_beat_packet(1, 0.0, Some(120.0), 1);
        bytes[0] = 0x00;
        assert!(parse_beat_message(&bytes).is_none());
    }

    #[test]
    fn rejects_an_unknown_packet_type() {
        let mut bytes = build_beat_packet(1, 0.0, Some(120.0), 1);
        bytes[PROLINK_HEADER.len()] = 0x99;
        assert!(parse_beat_message(&bytes).is_none());
    }

    #[test]
    fn rejects_a_truncated_beat_packet() {
        let mut bytes = build_beat_packet(1, 0.0, Some(120.0), 1);
        bytes.truncate(BEAT_PACKET_LEN - 1);
        assert!(parse_beat_message(&bytes).is_none());
    }
}
