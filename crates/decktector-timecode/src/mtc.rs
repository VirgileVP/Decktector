//! MIDI Time Code (MTC) encoding and output.
//!
//! <https://en.wikipedia.org/wiki/MIDI_timecode> — unlike Art-Net TC, MTC has
//! no "one packet, one frame" message: a full timecode update is 8
//! quarter-frame messages (status `0xF1`, one data byte each), spread over 2
//! frames of wall-clock time. [`QuarterFrameCycle`] tracks progress through
//! that 8-message cycle; the tick loop in `lib.rs` is what decides how often
//! to call it (every `1/fps/4` seconds, a quarter of the Art-Net cadence).
//!
//! A Full Timecode SysEx message (a single message carrying the complete
//! `HH:MM:SS:FF`) is sent separately whenever output (re)starts, so a
//! receiver can lock immediately instead of waiting up to 2 frames for the
//! quarter-frame stream to complete a cycle.

use midir::{MidiOutput, MidiOutputConnection};

use crate::{FrameRate, TimeCode};

/// Client name MTC output registers under with the OS's MIDI subsystem.
const CLIENT_NAME: &str = "Decktector";
/// Name of the connection this crate opens to a chosen output port — shown
/// by some MIDI monitors alongside the port itself.
const CONNECTION_NAME: &str = "decktector-timecode";

const QUARTER_FRAME_STATUS: u8 = 0xF1;

/// Every MIDI output port this machine could send MTC to.
pub fn list_output_ports() -> anyhow::Result<Vec<String>> {
    let output = MidiOutput::new(CLIENT_NAME)?;
    output
        .ports()
        .iter()
        .map(|port| output.port_name(port).map_err(anyhow::Error::from))
        .collect()
}

/// Opens a connection to the named output port.
///
/// Re-resolved on every call rather than cached indefinitely: a port name is
/// only stable for the lifetime of the device/driver behind it, and the
/// caller (`lib.rs`'s tick loop) already caches the resulting connection
/// itself, the same way it caches the Art-Net socket.
pub(crate) fn connect(port_name: &str) -> anyhow::Result<MidiOutputConnection> {
    let output = MidiOutput::new(CLIENT_NAME)?;
    let port = output
        .ports()
        .into_iter()
        .find(|candidate| {
            output
                .port_name(candidate)
                .map(|name| name == port_name)
                .unwrap_or(false)
        })
        .ok_or_else(|| anyhow::anyhow!("MIDI output port '{port_name}' not found"))?;
    output.connect(&port, CONNECTION_NAME).map_err(|error| {
        anyhow::anyhow!("failed to connect to MIDI output port '{port_name}': {error}")
    })
}

/// Sends raw MIDI bytes (a quarter-frame message or the Full Timecode SysEx)
/// over an already-connected output port.
///
/// Like [`crate::artnet::send`], a failure here means one dropped MIDI
/// message, not a broken session — logged and swallowed rather than
/// propagated into the tick loop.
pub(crate) fn send(connection: &mut MidiOutputConnection, bytes: &[u8]) {
    if let Err(error) = connection.send(bytes) {
        tracing::warn!(%error, "failed to send an MTC message");
    }
}

/// One tick's worth of quarter-frame work: the MIDI message to send, the
/// timecode it describes, and whether this tick just latched a new value
/// (piece 0 of a fresh cycle) — the signal the caller uses to know when to
/// re-broadcast [`crate::TimecodeStatus`].
pub(crate) struct QuarterFrameTick {
    pub(crate) bytes: [u8; 2],
    pub(crate) timecode: TimeCode,
    pub(crate) is_new_cycle: bool,
}

/// Tracks progress through one 8-message MTC quarter-frame cycle.
///
/// A full timecode update takes 8 quarter-frame messages, covering 2 frames
/// of wall-clock time — the value being described is latched once at the
/// start of each cycle (piece 0) so all 8 messages agree on a single instant
/// instead of drifting as the underlying playhead keeps moving mid-cycle.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct QuarterFrameCycle {
    piece: u8,
    latched: TimeCode,
}

impl QuarterFrameCycle {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Advances one tick and returns the message for the current piece.
    pub(crate) fn tick(&mut self, current: TimeCode, frame_rate: FrameRate) -> QuarterFrameTick {
        let is_new_cycle = self.piece == 0;
        if is_new_cycle {
            self.latched = current;
        }
        let bytes = quarter_frame_bytes(self.piece, self.latched, frame_rate);
        let timecode = self.latched;
        self.piece = (self.piece + 1) % 8;
        QuarterFrameTick {
            bytes,
            timecode,
            is_new_cycle,
        }
    }
}

/// The `0xF1` quarter-frame message for `piece` (0..8) of `timecode`.
fn quarter_frame_bytes(piece: u8, timecode: TimeCode, frame_rate: FrameRate) -> [u8; 2] {
    let nibble = match piece {
        0 => timecode.frames & 0x0F,
        1 => (timecode.frames >> 4) & 0x01,
        2 => timecode.seconds & 0x0F,
        3 => (timecode.seconds >> 4) & 0x03,
        4 => timecode.minutes & 0x0F,
        5 => (timecode.minutes >> 4) & 0x03,
        6 => timecode.hours & 0x0F,
        7 => (frame_rate.mtc_rate_code() << 1) | ((timecode.hours >> 4) & 0x01),
        _ => unreachable!("quarter-frame piece index is always 0..8"),
    };
    [QUARTER_FRAME_STATUS, (piece << 4) | nibble]
}

/// The Full Timecode SysEx message for `timecode` — a single message
/// carrying the complete `HH:MM:SS:FF`, used to let a receiver lock
/// immediately rather than waiting on the quarter-frame stream.
pub(crate) fn full_timecode_sysex(timecode: TimeCode, frame_rate: FrameRate) -> [u8; 10] {
    let hours_byte = (frame_rate.mtc_rate_code() << 5) | (timecode.hours & 0x1F);
    [
        0xF0,
        0x7F,
        0x7F,
        0x01,
        0x01,
        hours_byte,
        timecode.minutes,
        timecode.seconds,
        timecode.frames,
        0xF7,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_timecode() -> TimeCode {
        TimeCode {
            hours: 13,
            minutes: 45,
            seconds: 52,
            frames: 21,
        }
    }

    #[test]
    fn quarter_frame_bytes_encode_each_piece_of_a_known_timecode() {
        let timecode = sample_timecode();
        let expected: [(u8, u8); 8] = [
            (0, 0x05),
            (1, 0x11),
            (2, 0x24),
            (3, 0x33),
            (4, 0x4D),
            (5, 0x52),
            (6, 0x6D),
            (7, 0x76),
        ];
        for (piece, data) in expected {
            let bytes = quarter_frame_bytes(piece, timecode, FrameRate::Smpte30);
            assert_eq!(bytes, [QUARTER_FRAME_STATUS, data], "piece {piece}");
        }
    }

    #[test]
    fn full_timecode_sysex_carries_the_rate_code_and_timecode_fields() {
        let sysex = full_timecode_sysex(sample_timecode(), FrameRate::Smpte30);
        assert_eq!(
            sysex,
            [0xF0, 0x7F, 0x7F, 0x01, 0x01, 0x6D, 0x2D, 0x34, 0x15, 0xF7]
        );
    }

    #[test]
    fn quarter_frame_cycle_latches_once_per_eight_ticks_and_covers_every_piece() {
        let mut cycle = QuarterFrameCycle::new();
        let first = sample_timecode();
        let second = TimeCode {
            frames: first.frames + 1,
            ..first
        };

        // The first 8 ticks describe `first`, even if the caller starts
        // reporting `second` partway through the cycle.
        for piece in 0..8u8 {
            let current = if piece == 0 { first } else { second };
            let tick = cycle.tick(current, FrameRate::Smpte30);
            assert_eq!(tick.timecode, first, "piece {piece} mid-cycle");
            assert_eq!(tick.is_new_cycle, piece == 0, "piece {piece} cycle flag");
        }

        // The 9th tick starts a new cycle and re-latches.
        let tick = cycle.tick(second, FrameRate::Smpte30);
        assert_eq!(tick.timecode, second);
        assert!(tick.is_new_cycle);
    }
}
