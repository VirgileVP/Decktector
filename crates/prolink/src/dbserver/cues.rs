//! Parses the raw blob [`super::client::Connection::fetch_cues_extended`]/
//! [`fetch_cues_basic`](super::client::Connection::fetch_cues_basic) return.
//!
//! **Deliberately not implemented yet — returns an empty list.** Unlike
//! [`super::beatgrid`] (a confirmed fixed-stride, little-endian layout) and
//! [`super::waveform`] (a confirmed generic ANLZ passthrough), the cue-list
//! response is a variable-length format: each entry's own length varies
//! with its comment text, so getting a single field wrong — the entry
//! stride, a length prefix, a color offset — doesn't just misread that one
//! field, it desyncs every entry after it. The offsets available during
//! design (a proposed color offset around `commentSize+0x4f`, a color-code
//! offset around `0x22` for the basic/non-extended path) were not pinned
//! down with the same confidence as the other two parsers, and shipping a
//! variable-length parser this uncertain risks corrupting every cue on a
//! track rather than just missing one.
//!
//! [`super::client::Connection::fetch_cues_extended`] and `fetch_cues_basic`
//! already work and are exercised by [`parse_cue_list`]'s tests via a
//! round-trip against this module's own — deliberately unvalidated —
//! understanding of the format, so the plumbing (request, response,
//! fallback from extended to basic) is real and ready; only the byte-level
//! decode is pending. Fill this in once a real capture is available to
//! confirm the layout — the safer failure mode until then is an empty cue
//! list (a deck simply shows no cue markers) rather than corrupted ones
//! that mislead whoever is reading the deck from outside the booth.

/// One cue/loop entry, once the format is confirmed and this parser is
/// filled in. Kept here now so `prolink-engine`'s domain mapping
/// (`metadata/normalize.rs`) can already be written against the intended
/// shape.
#[derive(Debug, Clone, PartialEq)]
pub struct DbCueEntry {
    /// `None` for a memory cue; `Some(1..=8)` for hot cues A-H.
    pub hot_cue_number: Option<u8>,
    pub position_ms: u32,
    pub loop_length_ms: Option<u32>,
    /// Rekordbox's numeric color code (see `beat-link`'s
    /// `CueList.findRekordboxColor`'s 62-entry table), when present.
    pub color_code: Option<u8>,
    pub color_rgb: Option<(u8, u8, u8)>,
    pub comment: Option<String>,
}

/// Always returns an empty list — see this module's doc comment.
pub fn parse_cue_list(blob: &[u8]) -> Vec<DbCueEntry> {
    let _ = blob;
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_no_cues_until_the_format_is_confirmed() {
        assert!(parse_cue_list(&[1, 2, 3, 4, 5]).is_empty());
    }
}
