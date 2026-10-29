//! Player status packets: what a CDJ/XDJ broadcasts about itself roughly
//! five times a second — play/master/sync/on-air state, pitch, tempo, beat
//! position and the loaded track's identity.
//!
//! Byte offsets come from Deep Symmetry's protocol analysis
//! (<https://djl-analysis.deepsymmetry.org/djl-analysis/vcdj.html>), cross
//! -checked against decktector's pre-Rust engine (which read the same
//! packet through the `prolink-connect` library, verified against real
//! hardware over a full development cycle) to make sure the offsets used
//! here are the ones that actually mattered in practice, not just what the
//! docs mention. Packet length varies by hardware generation (0xd0 bytes on
//! older CDJs up to 0x200 on a CDJ-3000); every field this parser reads
//! sits well within the shortest known packet, so length is checked as a
//! floor, not an exact match like
//! [`crate::discovery::parse_keepalive`]'s keep-alive packet.

use std::net::IpAddr;
use std::sync::Arc;

use tokio::sync::broadcast;

use crate::{bind_udp_socket, decode_pitch, PROLINK_HEADER, STATUS_PORT};

/// How many status updates the broadcast channel buffers per subscriber.
/// Status arrives at ~5 Hz per device; six players is ~30/s, so this holds a
/// couple of seconds of backlog. A subscriber that falls behind further
/// should re-hydrate from elsewhere rather than try to catch up.
const EVENT_BUFFER: usize = 256;

const STATUS_PACKET_TYPE: u8 = 0x0a;
/// Shortest status packet across hardware generations (older CDJs). Newer
/// firmware and CDJ-3000 send longer packets; every offset below is well
/// inside this floor.
const STATUS_PACKET_MIN_LEN: usize = 0xd0;

const DEVICE_NUMBER_OFFSET: usize = 0x21;
const TRACK_DEVICE_OFFSET: usize = 0x28;
const MEDIA_SLOT_OFFSET: usize = 0x29;
const TRACK_TYPE_OFFSET: usize = 0x2a;
const REKORDBOX_ID_OFFSET: usize = 0x2c;
const PLAY_STATE_OFFSET: usize = 0x7b;
const PITCH_OFFSET: usize = 0x8c;
const STATUS_FLAGS_OFFSET: usize = 0x89;
const BPM_OFFSET: usize = 0x92;
const BEAT_COUNTER_OFFSET: usize = 0xa0;
const BEAT_IN_BAR_OFFSET: usize = 0xa6;
/// Loop bounds, sent only by a CDJ-3000 (whose packets are 0x200 bytes) and
/// non-zero only while it is playing a loop.
const LOOP_START_OFFSET: usize = 0x1b6;
const LOOP_END_OFFSET: usize = 0x1be;
/// A packet must reach past the loop end field to carry loop bounds.
const LOOP_FIELDS_END: usize = LOOP_END_OFFSET + 4;
/// Length of a CDJ-3000 status packet.
const CDJ3000_PACKET_LEN: usize = 0x200;
/// Loop positions are sent in units of 65.536 ms (`value * 65536 / 1000`).
const LOOP_POSITION_UNIT_MS: f64 = 65_536.0 / 1_000.0;

const FLAG_MASTER: u8 = 0x20;
const FLAG_SYNCED: u8 = 0x10;
const FLAG_ON_AIR: u8 = 0x08;

/// `0xffff`/`0xffff_ffff` sentinels the protocol uses for "no track loaded"
/// on the BPM and beat-counter fields.
const BPM_UNKNOWN: u16 = 0xffff;
const BEAT_COUNT_UNKNOWN: u32 = 0xffff_ffff;

/// What slot a track was loaded from — mirrors the byte at
/// [`MEDIA_SLOT_OFFSET`]; kept as the raw protocol concept rather than
/// mapped to a domain type here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaSlot {
    Cd,
    Sd,
    Usb,
    Rekordbox,
    Streaming,
    Unknown(u8),
}

impl MediaSlot {
    fn from_byte(byte: u8) -> Self {
        match byte {
            0x01 => Self::Cd,
            0x02 => Self::Sd,
            0x03 => Self::Usb,
            0x04 => Self::Rekordbox,
            0x06 => Self::Streaming,
            other => Self::Unknown(other),
        }
    }

    /// The raw byte back — dbserver queries (`prolink-engine::metadata`)
    /// need to send the slot as part of their request, so decoding into
    /// this enum and then straight back to a byte is cheaper than plumbing
    /// the original byte through everywhere alongside the parsed form.
    pub fn to_byte(self) -> u8 {
        match self {
            Self::Cd => 0x01,
            Self::Sd => 0x02,
            Self::Usb => 0x03,
            Self::Rekordbox => 0x04,
            Self::Streaming => 0x06,
            Self::Unknown(byte) => byte,
        }
    }
}

/// The player's own play-mode byte (`P1` in Deep Symmetry's naming, offset
/// [`PLAY_STATE_OFFSET`]) — Pioneer's actual state names, richer than
/// decktector's own `PlayState`, which folds several of these together
/// (see `prolink-engine::normalize`, and decktector's pre-Rust engine's
/// `normalize.ts`, which folds the same `prolink-connect`-decoded states the
/// same way). Kept at this fidelity here rather than pre-folded, since it's
/// what the protocol actually reports and folding is a domain-mapping
/// decision, not a protocol one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawPlayState {
    NoTrack,
    Loading,
    Playing,
    Looping,
    /// Paused somewhere other than the cue point.
    PausedAway,
    /// Paused exactly at the cue point.
    PausedAtCue,
    CuePlaying,
    CueScratching,
    Searching,
    /// A CD that spun down from lack of use — not relevant to USB/SD/
    /// rekordbox media, but a real value the byte can carry.
    SpunDown,
    Ended,
    EmergencyLoop,
    /// A value this parser doesn't recognize — normalize.rs treats this the
    /// same way decktector's pre-Rust engine treated any state its own
    /// lookup table didn't cover: as [`decktector_core`]'s `PlayState::Empty`,
    /// the safest default when the meaning is genuinely unknown.
    Unknown(u8),
}

impl RawPlayState {
    fn from_byte(byte: u8) -> Self {
        match byte {
            0x00 => Self::NoTrack,
            0x02 => Self::Loading,
            0x03 => Self::Playing,
            0x04 => Self::Looping,
            0x05 => Self::PausedAway,
            0x06 => Self::PausedAtCue,
            0x07 => Self::CuePlaying,
            0x08 => Self::CueScratching,
            0x09 => Self::Searching,
            0x0e => Self::SpunDown,
            0x11 => Self::Ended,
            0x12 => Self::EmergencyLoop,
            other => Self::Unknown(other),
        }
    }

    fn to_byte(self) -> u8 {
        match self {
            Self::NoTrack => 0x00,
            Self::Loading => 0x02,
            Self::Playing => 0x03,
            Self::Looping => 0x04,
            Self::PausedAway => 0x05,
            Self::PausedAtCue => 0x06,
            Self::CuePlaying => 0x07,
            Self::CueScratching => 0x08,
            Self::Searching => 0x09,
            Self::SpunDown => 0x0e,
            Self::Ended => 0x11,
            Self::EmergencyLoop => 0x12,
            Self::Unknown(byte) => byte,
        }
    }
}

/// Where a loop a player is playing starts and ends, in ms from the start of
/// the track.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoopBounds {
    pub start_ms: f64,
    pub end_ms: f64,
}

/// One player's self-reported state, as reconstructed from its most recent
/// status packet.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeviceStatus {
    pub device_number: u8,
    /// Device number of the player whose media the loaded track came from
    /// — **not** always [`Self::device_number`]. On a linked setup, one
    /// player's USB feeds every deck: a CDJ-3000 playing a track off its
    /// neighbour's stick reports that neighbour here, and metadata for the
    /// track has to be asked of *that* player, in that player's slot, by
    /// that player's number. Zero when nothing is loaded.
    pub track_source_device: u8,
    pub play_state: RawPlayState,
    pub is_master: bool,
    pub is_synced: bool,
    pub is_on_air: bool,
    pub media_slot: MediaSlot,
    /// Raw track-type byte (rekordbox-analyzed vs. other) — passed through
    /// un-interpreted, since the only current consumer
    /// (`prolink-engine::metadata`'s dbserver queries) needs to send this
    /// exact byte back, not a decoded meaning.
    pub track_type: u8,
    /// Pitch fader as a percentage, e.g. `2.35` for +2.35%.
    pub pitch_percent: f64,
    /// BPM of the loaded track at 0% pitch; `None` when no track is loaded.
    pub track_bpm: Option<f64>,
    /// Beats elapsed since the track started; `None` when no track is
    /// loaded. This is the only position signal this hardware generation
    /// reports — there is no absolute playhead in a status packet.
    pub beat_count: Option<u32>,
    /// Position within the bar, 1-4; `None` when the byte read is outside
    /// that range (a player reports a sentinel rather than 1-4 when no beat
    /// grid position applies — validating the range here is cheap insurance
    /// against relying on a specific sentinel value that isn't fully
    /// pinned down).
    pub beat_in_bar: Option<u8>,
    /// Rekordbox track id; `None` when no track is loaded.
    pub track_id: Option<u32>,
    /// The loop being played; `None` when the player isn't looping, or is
    /// older than a CDJ-3000 and doesn't report loop bounds at all (it still
    /// says it is looping through [`Self::play_state`]).
    pub loop_bounds: Option<LoopBounds>,
}

/// [`DeviceStatus::track_type`] of a track analysed by rekordbox and listed
/// in the media's track database. The other kinds (an unanalysed file
/// loaded from the file browser, an audio CD track) carry an id that is not
/// a row of that database.
pub const TRACK_TYPE_REKORDBOX: u8 = 0x01;

/// [`DeviceStatus::track_type`] of a file loaded from the player's file
/// browser without rekordbox analysis. The player analyses it itself after
/// loading it, so its beat grid and waveform only appear some time later.
pub const TRACK_TYPE_UNANALYZED: u8 = 0x02;

/// [`DeviceStatus::track_type`] of a streamed track (Beatport Link,
/// Streaming Direct Play). Not on any media, but a player still answers
/// for it like for a rekordbox track (`beat-link`'s `CdjStatus.TrackType`).
pub const TRACK_TYPE_STREAMING: u8 = 0x06;

/// Parses a status packet's payload; `None` if it isn't one.
pub fn parse_status(data: &[u8], source_ip: IpAddr) -> Option<DeviceStatus> {
    let _ = source_ip; // Not currently part of the parsed shape; kept for a uniform *Watcher call shape.
    if data.len() < STATUS_PACKET_MIN_LEN
        || !data.starts_with(&PROLINK_HEADER)
        || data[PROLINK_HEADER.len()] != STATUS_PACKET_TYPE
    {
        return None;
    }

    let flags = data[STATUS_FLAGS_OFFSET];
    let rekordbox_id = read_u32(data, REKORDBOX_ID_OFFSET);
    let bpm_raw = read_u16(data, BPM_OFFSET);
    let beat_count_raw = read_u32(data, BEAT_COUNTER_OFFSET);
    let beat_in_bar_raw = data[BEAT_IN_BAR_OFFSET];

    Some(DeviceStatus {
        device_number: data[DEVICE_NUMBER_OFFSET],
        track_source_device: data[TRACK_DEVICE_OFFSET],
        play_state: RawPlayState::from_byte(data[PLAY_STATE_OFFSET]),
        is_master: flags & FLAG_MASTER != 0,
        is_synced: flags & FLAG_SYNCED != 0,
        is_on_air: flags & FLAG_ON_AIR != 0,
        media_slot: MediaSlot::from_byte(data[MEDIA_SLOT_OFFSET]),
        track_type: data[TRACK_TYPE_OFFSET],
        pitch_percent: decode_pitch(read_u32(data, PITCH_OFFSET)),
        track_bpm: (bpm_raw != BPM_UNKNOWN).then(|| f64::from(bpm_raw) / 100.0),
        beat_count: (beat_count_raw != BEAT_COUNT_UNKNOWN).then_some(beat_count_raw),
        beat_in_bar: (1..=4)
            .contains(&beat_in_bar_raw)
            .then_some(beat_in_bar_raw),
        track_id: (rekordbox_id != 0).then_some(rekordbox_id),
        loop_bounds: parse_loop_bounds(data),
    })
}

/// The loop bounds of a CDJ-3000 packet, `None` for a shorter packet or when
/// no loop is playing (both fields are then zero).
fn parse_loop_bounds(data: &[u8]) -> Option<LoopBounds> {
    if data.len() < LOOP_FIELDS_END {
        return None;
    }
    let end_raw = read_u32(data, LOOP_END_OFFSET);
    if end_raw == 0 {
        return None;
    }
    Some(LoopBounds {
        start_ms: f64::from(read_u32(data, LOOP_START_OFFSET)) * LOOP_POSITION_UNIT_MS,
        end_ms: f64::from(end_raw) * LOOP_POSITION_UNIT_MS,
    })
}

/// Grows a packet from [`build_status`] to a CDJ-3000's length and writes
/// `bounds` into its loop fields, as a CDJ-3000 playing that loop would.
pub fn write_loop_bounds(packet: &mut Vec<u8>, bounds: LoopBounds) {
    if packet.len() < CDJ3000_PACKET_LEN {
        packet.resize(CDJ3000_PACKET_LEN, 0);
    }
    let to_raw = |ms: f64| (ms / LOOP_POSITION_UNIT_MS).round() as u32;
    write_u32(packet, LOOP_START_OFFSET, to_raw(bounds.start_ms));
    write_u32(packet, LOOP_END_OFFSET, to_raw(bounds.end_ms));
}

/// Builds a well-formed status packet for the given fields, zero-filling
/// everything this parser doesn't read. Used by tests and by the `cdj-sim`
/// dev tool. `device_number` also fills [`TRACK_DEVICE_OFFSET`] so a
/// simulated player is self-consistent about which device its "loaded from"
/// byte points at, mirroring how a real player only ever loads its own slot
/// during normal (non-linked) use.
#[allow(clippy::too_many_arguments)]
pub fn build_status(
    device_number: u8,
    play_state: RawPlayState,
    is_master: bool,
    is_synced: bool,
    is_on_air: bool,
    pitch_percent: f64,
    track_bpm: Option<f64>,
    beat_count: Option<u32>,
    beat_in_bar: Option<u8>,
    track_id: Option<u32>,
) -> Vec<u8> {
    let mut packet = vec![0u8; 0xd4];
    packet[..PROLINK_HEADER.len()].copy_from_slice(&PROLINK_HEADER);
    packet[PROLINK_HEADER.len()] = STATUS_PACKET_TYPE;
    packet[DEVICE_NUMBER_OFFSET] = device_number;
    packet[TRACK_DEVICE_OFFSET] = device_number;
    packet[MEDIA_SLOT_OFFSET] = 0x03; // USB, the common case for tests/sim.
    packet[TRACK_TYPE_OFFSET] = TRACK_TYPE_REKORDBOX;
    packet[PLAY_STATE_OFFSET] = play_state.to_byte();

    let mut flags = 0u8;
    if is_master {
        flags |= FLAG_MASTER;
    }
    if is_synced {
        flags |= FLAG_SYNCED;
    }
    if is_on_air {
        flags |= FLAG_ON_AIR;
    }
    packet[STATUS_FLAGS_OFFSET] = flags;

    let pitch_raw = ((pitch_percent / 100.0 + 1.0) * f64::from(0x0010_0000u32)) as u32;
    write_u32(&mut packet, PITCH_OFFSET, pitch_raw);

    write_u16(
        &mut packet,
        BPM_OFFSET,
        track_bpm.map_or(BPM_UNKNOWN, |bpm| (bpm * 100.0) as u16),
    );
    write_u32(
        &mut packet,
        BEAT_COUNTER_OFFSET,
        beat_count.unwrap_or(BEAT_COUNT_UNKNOWN),
    );
    // 0 is outside parse_status's accepted 1-4 range, so it reads back as
    // "unknown" — the same sentinel real hardware is believed to use.
    packet[BEAT_IN_BAR_OFFSET] = beat_in_bar.unwrap_or(0);
    write_u32(&mut packet, REKORDBOX_ID_OFFSET, track_id.unwrap_or(0));

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

/// Listens for player status packets and republishes each one as it arrives.
///
/// Unlike [`crate::discovery::DeviceWatcher`], this does not coalesce into a
/// current-state list — status arrives fast enough (~5 Hz/device) and the
/// caller (`prolink-engine`) already has its own snapshot layer, so every
/// parsed packet is simply forwarded.
pub struct StatusWatcher {
    status_tx: broadcast::Sender<DeviceStatus>,
}

impl StatusWatcher {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            status_tx: broadcast::channel(EVENT_BUFFER).0,
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<DeviceStatus> {
        self.status_tx.subscribe()
    }

    /// Runs until the socket errors out or the process exits. A bind failure
    /// is logged and this watcher simply never reports anything, rather than
    /// taking the engine down over one socket — mirrors
    /// [`crate::discovery::DeviceWatcher::run`].
    pub async fn run(self: Arc<Self>) {
        let socket = match bind_udp_socket(STATUS_PORT) {
            Ok(socket) => socket,
            Err(error) => {
                tracing::error!(%error, "failed to bind the Pro DJ Link status socket");
                return;
            }
        };

        let mut buf = [0u8; 2048];
        loop {
            let (len, source) = match socket.recv_from(&mut buf).await {
                Ok(value) => value,
                Err(error) => {
                    tracing::warn!(%error, "failed to read from the status socket");
                    continue;
                }
            };
            if let Some(status) = parse_status(&buf[..len], source.ip()) {
                // No subscribers yet (engine still starting) is not an error.
                let _ = self.status_tx.send(status);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn source() -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(192, 168, 1, 50))
    }

    fn looping_packet() -> Vec<u8> {
        build_status(
            3,
            RawPlayState::Looping,
            false,
            false,
            true,
            0.0,
            Some(124.0),
            Some(32),
            Some(1),
            Some(12),
        )
    }

    #[test]
    fn a_cdj_3000_packet_carries_the_loop_it_is_playing() {
        let mut bytes = looping_packet();
        // 1_000 and 1_030 units of 65.536 ms: 65.536 s to 67.502 s.
        bytes.resize(CDJ3000_PACKET_LEN, 0);
        write_u32(&mut bytes, LOOP_START_OFFSET, 1_000);
        write_u32(&mut bytes, LOOP_END_OFFSET, 1_030);

        let bounds = parse_status(&bytes, source())
            .and_then(|status| status.loop_bounds)
            .expect("loop bounds");

        assert!((bounds.start_ms - 65_536.0).abs() < 1e-6);
        assert!((bounds.end_ms - 67_502.08).abs() < 1e-6);
    }

    #[test]
    fn written_loop_bounds_read_back_to_the_nearest_unit() {
        let mut bytes = looping_packet();
        write_loop_bounds(
            &mut bytes,
            LoopBounds {
                start_ms: 10_000.0,
                end_ms: 12_000.0,
            },
        );

        let bounds = parse_status(&bytes, source())
            .and_then(|status| status.loop_bounds)
            .expect("loop bounds");

        assert_eq!(bytes.len(), CDJ3000_PACKET_LEN);
        assert!((bounds.start_ms - 10_000.0).abs() <= LOOP_POSITION_UNIT_MS / 2.0);
        assert!((bounds.end_ms - 12_000.0).abs() <= LOOP_POSITION_UNIT_MS / 2.0);
    }

    #[test]
    fn a_cdj_3000_packet_outside_a_loop_has_no_loop_bounds() {
        let mut bytes = looping_packet();
        bytes.resize(CDJ3000_PACKET_LEN, 0);

        let status = parse_status(&bytes, source()).expect("should parse");

        assert_eq!(status.loop_bounds, None);
    }

    #[test]
    fn an_older_players_packet_has_no_loop_bounds_even_while_looping() {
        let status = parse_status(&looping_packet(), source()).expect("should parse");

        assert_eq!(status.play_state, RawPlayState::Looping);
        assert_eq!(status.loop_bounds, None);
    }

    #[test]
    fn parses_a_playing_master_deck() {
        let bytes = build_status(
            2,
            RawPlayState::Playing,
            true,
            false,
            true,
            2.35,
            Some(128.0),
            Some(64),
            Some(1),
            Some(4207),
        );
        let status = parse_status(&bytes, source()).expect("should parse");

        assert_eq!(status.device_number, 2);
        assert_eq!(status.play_state, RawPlayState::Playing);
        assert!(status.is_master);
        assert!(!status.is_synced);
        assert!(status.is_on_air);
        assert_eq!(status.media_slot, MediaSlot::Usb);
        assert!((status.pitch_percent - 2.35).abs() < 0.01);
        assert_eq!(status.track_bpm, Some(128.0));
        assert_eq!(status.beat_count, Some(64));
        assert_eq!(status.beat_in_bar, Some(1));
        assert_eq!(status.track_id, Some(4207));
    }

    #[test]
    fn an_empty_deck_reports_no_track_fields() {
        let bytes = build_status(
            3,
            RawPlayState::NoTrack,
            false,
            false,
            false,
            0.0,
            None,
            None,
            None,
            None,
        );
        let status = parse_status(&bytes, source()).expect("should parse");

        assert_eq!(status.play_state, RawPlayState::NoTrack);
        assert_eq!(status.track_bpm, None);
        assert_eq!(status.beat_count, None);
        assert_eq!(status.beat_in_bar, None);
        assert_eq!(status.track_id, None);
    }

    #[test]
    fn every_known_play_state_byte_round_trips() {
        let states = [
            RawPlayState::NoTrack,
            RawPlayState::Loading,
            RawPlayState::Playing,
            RawPlayState::Looping,
            RawPlayState::PausedAway,
            RawPlayState::PausedAtCue,
            RawPlayState::CuePlaying,
            RawPlayState::CueScratching,
            RawPlayState::Searching,
            RawPlayState::SpunDown,
            RawPlayState::Ended,
            RawPlayState::EmergencyLoop,
        ];
        for state in states {
            let bytes = build_status(1, state, false, false, false, 0.0, None, None, None, None);
            let status = parse_status(&bytes, source()).expect("should parse");
            assert_eq!(status.play_state, state);
        }
    }

    #[test]
    fn an_unrecognized_play_state_byte_is_preserved_as_unknown() {
        let bytes = build_status(
            1,
            RawPlayState::Unknown(0x7f),
            false,
            false,
            false,
            0.0,
            None,
            None,
            None,
            None,
        );
        let status = parse_status(&bytes, source()).expect("should parse");
        assert_eq!(status.play_state, RawPlayState::Unknown(0x7f));
    }

    #[test]
    fn a_beat_in_bar_outside_one_to_four_reads_as_unknown() {
        let mut bytes = build_status(
            1,
            RawPlayState::Playing,
            false,
            false,
            false,
            0.0,
            Some(120.0),
            Some(10),
            None,
            Some(1),
        );
        bytes[BEAT_IN_BAR_OFFSET] = 0xff;
        let status = parse_status(&bytes, source()).expect("should parse");
        assert_eq!(status.beat_in_bar, None);
    }

    #[test]
    fn rejects_a_packet_shorter_than_the_known_floor() {
        let mut bytes = build_status(
            2,
            RawPlayState::Playing,
            false,
            false,
            false,
            0.0,
            Some(120.0),
            Some(1),
            Some(1),
            Some(1),
        );
        bytes.truncate(STATUS_PACKET_MIN_LEN - 1);
        assert!(parse_status(&bytes, source()).is_none());
    }

    #[test]
    fn accepts_a_longer_cdj_3000_style_packet() {
        let mut bytes = build_status(
            2,
            RawPlayState::Playing,
            false,
            false,
            false,
            0.0,
            Some(120.0),
            Some(1),
            Some(1),
            Some(1),
        );
        bytes.resize(0x200, 0);
        assert!(parse_status(&bytes, source()).is_some());
    }

    #[test]
    fn rejects_a_foreign_header() {
        let mut bytes = build_status(
            2,
            RawPlayState::Playing,
            false,
            false,
            false,
            0.0,
            None,
            None,
            None,
            None,
        );
        bytes[0] = 0x00;
        assert!(parse_status(&bytes, source()).is_none());
    }

    #[test]
    fn rejects_a_non_status_packet_type() {
        let mut bytes = build_status(
            2,
            RawPlayState::Playing,
            false,
            false,
            false,
            0.0,
            None,
            None,
            None,
            None,
        );
        bytes[PROLINK_HEADER.len()] = 0x06; // A keep-alive, not a status packet.
        assert!(parse_status(&bytes, source()).is_none());
    }
}
