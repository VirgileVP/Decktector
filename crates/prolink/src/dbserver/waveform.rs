//! Parses the raw blob [`super::client::Connection::fetch_anlz_tag`]
//! returns for a color-waveform-detail request.
//!
//! `0x2c04` (`ANLZ_TAG_REQ`) is a generic passthrough: it hands back the
//! literal bytes of the requested ANLZ file tag (Deep Symmetry's `beat-link`
//! confirms this by treating the two as bit-for-bit identical once the
//! response's own leading junk bytes are skipped), unlike beat grid or cue
//! list — see those modules' doc comments — which are older, dedicated
//! query types never migrated to this generic form.
//!
//! Requests this codebase's waveform fetch sends: `tag_fourcc` is the
//! `PWV5` (color waveform detail) tag magic, byte-reversed for the wire —
//! `0x35565750` — with `file_extension` the `EXT` extension packed the same
//! way, matching where Rekordbox stores color waveform data.

/// One waveform column, protocol-shaped — deliberately not
/// `decktector_core::model::WaveformSegment`: this crate stays free of
/// Decktector's domain model (see the crate doc comment), the same reason
/// [`super::beatgrid::DbBeatGridEntry`] isn't `BeatGridBeat`. The shape is
/// intentionally identical (`prolink-engine::metadata::normalize` maps it
/// field-for-field) so that mapping is a trivial, obviously-correct step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DbWaveformColumn {
    /// 0-1.
    pub height: f32,
    /// `[r, g, b]`, 0-255.
    pub color: [u8; 3],
}

/// The ANLZ tag magic for color waveform detail (`"PWV5"`, byte-reversed
/// for the wire), and the file extension it lives in — the two arguments
/// [`super::client::Connection::fetch_anlz_tag`] needs for a waveform
/// fetch.
pub const COLOR_WAVEFORM_DETAIL_TAG: u32 = 0x3556_5750;
/// `"EXT"` read as a little-endian number with a NUL fourth byte —
/// `beat-link`'s `Message.ALNZ_FILE_TYPE_EXT`. A number field, not a text
/// one: sent as the string `".EXT"`, the request was never one a player
/// recognises.
pub const COLOR_WAVEFORM_FILE_EXTENSION: u32 = 0x0054_5845;

/// Bytes to skip at the start of the response before the column data
/// starts — `beat-link`'s own documented, hardware-derived constant
/// (`LEADING_DBSERVER_COLOR_JUNK_BYTES`). Not independently re-derived here;
/// treated as correct until a real capture says otherwise.
const LEADING_JUNK_BYTES: usize = 28;
/// Each waveform column is a 2-byte bitfield.
const COLUMN_LEN: usize = 2;
/// HD waveform detail carries this many columns per second of audio.
pub const SEGMENTS_PER_SECOND: f64 = 150.0;

/// Decodes every complete column after the leading junk bytes. A
/// short/malformed trailing fragment is dropped rather than erroring — a
/// partial waveform renders as a partial waveform, not a missing one.
pub fn parse_color_waveform_detail(blob: &[u8]) -> Vec<DbWaveformColumn> {
    if blob.len() <= LEADING_JUNK_BYTES {
        return Vec::new();
    }
    blob[LEADING_JUNK_BYTES..]
        .as_chunks::<COLUMN_LEN>()
        .0
        .iter()
        .map(|column| {
            let bits = u16::from_be_bytes(*column);
            let height = f32::from((bits >> 2) & 0x1f) / 31.0;
            let scale_channel =
                |shift: u16| (f32::from((bits >> shift) & 0x7) / 7.0 * 255.0).round() as u8;
            DbWaveformColumn {
                height,
                color: [scale_channel(13), scale_channel(10), scale_channel(7)],
            }
        })
        .collect()
}

/// How many milliseconds one detail column spans — `TrackInfo`'s
/// `ms_per_detail_segment` field.
pub fn ms_per_segment() -> f64 {
    1000.0 / SEGMENTS_PER_SECOND
}

#[cfg(test)]
mod tests {
    use super::*;

    fn column_bytes(height5: u16, red3: u16, green3: u16, blue3: u16) -> [u8; 2] {
        let bits = (height5 << 2) | (red3 << 13) | (green3 << 10) | (blue3 << 7);
        bits.to_be_bytes()
    }

    #[test]
    fn decodes_a_full_height_white_column() {
        let mut blob = vec![0u8; LEADING_JUNK_BYTES];
        blob.extend_from_slice(&column_bytes(31, 7, 7, 7));

        let segments = parse_color_waveform_detail(&blob);
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].height, 1.0);
        assert_eq!(segments[0].color, [255, 255, 255]);
    }

    #[test]
    fn decodes_a_low_blue_leaning_column() {
        let mut blob = vec![0u8; LEADING_JUNK_BYTES];
        blob.extend_from_slice(&column_bytes(8, 0, 0, 7));

        let segments = parse_color_waveform_detail(&blob);
        assert!((segments[0].height - 8.0 / 31.0).abs() < 0.001);
        assert_eq!(segments[0].color, [0, 0, 255]);
    }

    #[test]
    fn a_blob_too_short_for_the_header_yields_no_segments() {
        assert!(parse_color_waveform_detail(&[0u8; 4]).is_empty());
    }

    #[test]
    fn ms_per_segment_matches_150_columns_per_second() {
        assert!((ms_per_segment() - 6.666_666_666_666_667).abs() < 1e-9);
    }
}
