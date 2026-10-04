//! Parses the raw blob [`super::client::Connection::fetch_beat_grid`]
//! returns.
//!
//! Byte layout per Deep Symmetry's `beat-link` (`BeatGrid`'s network
//! constructor, cross-checked during design): a 20-byte header, then
//! fixed-stride 16-byte entries — **little-endian**, unlike every UDP
//! packet field elsewhere in this codebase (`beat_in_bar: u16`,
//! `tempo_centibpm: u16`, `time_ms: u32`, 8 bytes of trailing padding this
//! parser doesn't need). This is a genuinely different wire format from the
//! on-disk ANLZ `PQTZ` beat-grid tag (big-endian, 12-byte header, 8-byte
//! entries) — the dbserver response is not byte-identical to the file
//! format here, unlike the waveform response (see [`super::waveform`]).

const HEADER_LEN: usize = 20;
const ENTRY_LEN: usize = 16;

/// One beat-grid entry as the dbserver reports it — protocol-shaped, not
/// yet `decktector_core::model::BeatGridBeat` (that mapping, including
/// computing each beat's offset in ms, needs `time_ms` directly, which this
/// format already gives per-entry, unlike the file format's beat-count
/// -plus-tempo encoding).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DbBeatGridEntry {
    /// Position within the bar, 1-4.
    pub beat_in_bar: u8,
    /// Tempo at this beat, BPM x100.
    pub tempo_centibpm: u16,
    pub time_ms: u32,
}

/// Parses every complete entry in `blob`; a short/malformed trailing
/// fragment is silently dropped rather than causing an error — a partial
/// beat grid is more useful than none, and this is diagnostic/visual data,
/// not something correctness-critical downstream depends on.
pub fn parse_beat_grid(blob: &[u8]) -> Vec<DbBeatGridEntry> {
    if blob.len() <= HEADER_LEN {
        return Vec::new();
    }
    blob[HEADER_LEN..]
        .as_chunks::<ENTRY_LEN>()
        .0
        .iter()
        .map(|entry| DbBeatGridEntry {
            beat_in_bar: u16::from_le_bytes([entry[0], entry[1]]) as u8,
            tempo_centibpm: u16::from_le_bytes([entry[2], entry[3]]),
            time_ms: u32::from_le_bytes([entry[4], entry[5], entry[6], entry[7]]),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry_bytes(beat_in_bar: u16, tempo_centibpm: u16, time_ms: u32) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(ENTRY_LEN);
        bytes.extend_from_slice(&beat_in_bar.to_le_bytes());
        bytes.extend_from_slice(&tempo_centibpm.to_le_bytes());
        bytes.extend_from_slice(&time_ms.to_le_bytes());
        bytes.extend_from_slice(&[0u8; 8]); // Padding this parser ignores.
        bytes
    }

    #[test]
    fn parses_entries_after_the_header() {
        let mut blob = vec![0u8; HEADER_LEN];
        blob.extend_from_slice(&entry_bytes(1, 12_800, 0));
        blob.extend_from_slice(&entry_bytes(2, 12_800, 469));

        let entries = parse_beat_grid(&blob);
        assert_eq!(entries.len(), 2);
        assert_eq!(
            entries[0],
            DbBeatGridEntry {
                beat_in_bar: 1,
                tempo_centibpm: 12_800,
                time_ms: 0
            }
        );
        assert_eq!(entries[1].time_ms, 469);
    }

    #[test]
    fn a_blob_too_short_for_the_header_yields_no_entries() {
        assert!(parse_beat_grid(&[0u8; 4]).is_empty());
    }

    #[test]
    fn a_trailing_partial_entry_is_dropped_rather_than_panicking() {
        let mut blob = vec![0u8; HEADER_LEN];
        blob.extend_from_slice(&entry_bytes(1, 12_800, 0));
        blob.extend_from_slice(&[0u8; 5]); // Shorter than one entry.

        assert_eq!(parse_beat_grid(&blob).len(), 1);
    }
}
