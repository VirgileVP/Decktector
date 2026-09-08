//! Binary decoders for ANLZ cue-list sections: `PCO2` (extended cues on
//! NXS2 / CDJ-3000) and `PCOB` (basic cues on classic CDJs).
//!
//! # Background
//!
//! Pioneer analysis files (`.EXT` and `.DAT`) store cues in tagged sections:
//!
//! - **`PCO2` (Extended Cue List)**: Introduced with the CDJ-2000NXS2 and used
//!   by the CDJ-3000. Carries up to 8 hot cues (A–H) or memory cues, loop ranges,
//!   DJ-assigned comments (UTF-16BE), hot-cue palette color codes, and full RGB
//!   pad illumination colors.
//! - **`PCOB` (Basic Cue List)**: The legacy format (pre-NXS2). Limited to 3
//!   hot cues (A–C) or memory cues, with no comments or color data.
//!
//! The published `rekordcrate 0.3` fails to parse `PCO2` sections because its
//! internal `CueType` enum only permits discriminants `0` (point) and `2` (loop),
//! whereas real CDJ-3000 / NXS2 hardware writes `1` for single cue points.
//! Furthermore, variable comment lengths and trailing color byte variations
//! caused whole-file deserialization failures. This module hand-rolls lenient,
//! robust parsers for both formats.

/// One decoded cue point or loop from an ANLZ section (`PCO2` or `PCOB`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnlzCue {
    /// `None` for a memory cue; `Some(1..=8)` for hot cues A through H.
    pub hot_cue: Option<u8>,
    /// Whether this cue represents a loop range.
    pub is_loop: bool,
    /// Playback offset in milliseconds where this cue occurs.
    pub time_ms: u32,
    /// Playback offset in milliseconds where the loop jumps back to `time_ms`.
    /// `None` if this cue is a point rather than a loop.
    pub loop_time_ms: Option<u32>,
    /// DJ-assigned comment label, if any (from `PCP2`).
    pub comment: Option<String>,
    /// Palette color code: hot-cue palette index (1..=62) or memory-cue index (1..=8).
    pub color_code: Option<u8>,
    /// Explicit RGB color `(R, G, B)` for pad lighting, when provided.
    pub color_rgb: Option<(u8, u8, u8)>,
}

const PCO2_MAGIC: &[u8; 4] = b"PCO2";
const PCP2_MAGIC: &[u8; 4] = b"PCP2";
const PCOB_MAGIC: &[u8; 4] = b"PCOB";
const PCPT_MAGIC: &[u8; 4] = b"PCPT";

/// Parses a `PCO2` section (header and `PCP2` entries).
pub fn parse_pco2(section_bytes: &[u8]) -> Vec<AnlzCue> {
    if section_bytes.len() < 20 || &section_bytes[..4] != PCO2_MAGIC {
        return Vec::new();
    }

    let header_size = u32::from_be_bytes([
        section_bytes[4],
        section_bytes[5],
        section_bytes[6],
        section_bytes[7],
    ]) as usize;
    let total_size = u32::from_be_bytes([
        section_bytes[8],
        section_bytes[9],
        section_bytes[10],
        section_bytes[11],
    ]) as usize;

    if header_size < 20 || section_bytes.len() < total_size {
        return Vec::new();
    }

    let num_cues = u16::from_be_bytes([section_bytes[16], section_bytes[17]]) as usize;
    let mut cues = Vec::with_capacity(num_cues);
    let mut offset = header_size;

    for _ in 0..num_cues {
        if offset + 12 > total_size {
            break;
        }
        let entry_slice = &section_bytes[offset..total_size];
        if entry_slice.len() < 12 || &entry_slice[..4] != PCP2_MAGIC {
            break;
        }

        let entry_len = u32::from_be_bytes([
            entry_slice[8],
            entry_slice[9],
            entry_slice[10],
            entry_slice[11],
        ]) as usize;
        if entry_len < 28 || entry_slice.len() < entry_len {
            break;
        }

        let entry = &entry_slice[..entry_len];
        if let Some(cue) = parse_pcp2_entry(entry) {
            cues.push(cue);
        }

        offset += entry_len;
    }

    cues
}

/// Decodes an individual `PCP2` entry.
fn parse_pcp2_entry(entry: &[u8]) -> Option<AnlzCue> {
    if entry.len() < 28 {
        return None;
    }

    let hot_cue_raw = u32::from_be_bytes([entry[12], entry[13], entry[14], entry[15]]);
    let hot_cue = match hot_cue_raw {
        0 => None,
        1..=8 => Some(hot_cue_raw as u8),
        other => {
            // Hot cue numbers outside 1..=8 fall back to raw number
            u8::try_from(other).ok()
        }
    };

    let cue_type = entry[16];
    let is_loop = cue_type == 2;

    let time_ms = u32::from_be_bytes([entry[20], entry[21], entry[22], entry[23]]);
    let loop_time_raw = u32::from_be_bytes([entry[24], entry[25], entry[26], entry[27]]);
    let loop_time_ms = if is_loop && loop_time_raw != u32::MAX {
        Some(loop_time_raw)
    } else {
        None
    };

    let color_id = if entry.len() > 28 {
        match entry[28] {
            0 => None,
            code => Some(code),
        }
    } else {
        None
    };

    let mut comment = None;
    let mut len_comment = 0usize;

    if entry.len() >= 44 {
        let comment_bytes_len =
            u32::from_be_bytes([entry[40], entry[41], entry[42], entry[43]]) as usize;
        if comment_bytes_len > 0 && entry.len() >= 44 + comment_bytes_len {
            len_comment = comment_bytes_len;
            let raw_comment = &entry[44..44 + comment_bytes_len];
            let u16_chars: Vec<u16> = raw_comment
                .chunks_exact(2)
                .map(|chunk| u16::from_be_bytes([chunk[0], chunk[1]]))
                .collect();
            let text = String::from_utf16_lossy(&u16_chars);
            let trimmed = text.trim_end_matches('\0').trim();
            if !trimmed.is_empty() {
                comment = Some(trimmed.to_string());
            }
        }
    }

    let mut color_code = None;
    let mut color_rgb = None;

    let after_comment = 44 + len_comment;
    if entry.len() > after_comment {
        let code = entry[after_comment];
        if code != 0 {
            color_code = Some(code);
        }

        if entry.len() >= after_comment + 4 {
            let rgb = (
                entry[after_comment + 1],
                entry[after_comment + 2],
                entry[after_comment + 3],
            );
            if rgb != (0, 0, 0) {
                color_rgb = Some(rgb);
            }
        }
    }

    // If color_code was not found in the hot cue position, use color_id for memory cues
    let final_color_code = color_code.or(color_id);

    Some(AnlzCue {
        hot_cue,
        is_loop,
        time_ms,
        loop_time_ms,
        comment,
        color_code: final_color_code,
        color_rgb,
    })
}

/// Parses a legacy `PCOB` section (header and `PCPT` entries).
pub fn parse_pcob(section_bytes: &[u8]) -> Vec<AnlzCue> {
    if section_bytes.len() < 24 || &section_bytes[..4] != PCOB_MAGIC {
        return Vec::new();
    }

    let header_size = u32::from_be_bytes([
        section_bytes[4],
        section_bytes[5],
        section_bytes[6],
        section_bytes[7],
    ]) as usize;
    let total_size = u32::from_be_bytes([
        section_bytes[8],
        section_bytes[9],
        section_bytes[10],
        section_bytes[11],
    ]) as usize;

    if header_size < 24 || section_bytes.len() < total_size {
        return Vec::new();
    }

    let num_cues = u16::from_be_bytes([section_bytes[18], section_bytes[19]]) as usize;
    let mut cues = Vec::with_capacity(num_cues);
    let mut offset = header_size;

    for _ in 0..num_cues {
        if offset + 12 > total_size {
            break;
        }
        let entry_slice = &section_bytes[offset..total_size];
        if entry_slice.len() < 12 || &entry_slice[..4] != PCPT_MAGIC {
            break;
        }

        let entry_len = u32::from_be_bytes([
            entry_slice[8],
            entry_slice[9],
            entry_slice[10],
            entry_slice[11],
        ]) as usize;
        if entry_len < 40 || entry_slice.len() < entry_len {
            break;
        }

        let entry = &entry_slice[..entry_len];
        if let Some(cue) = parse_pcpt_entry(entry) {
            cues.push(cue);
        }

        offset += entry_len;
    }

    cues
}

/// Decodes an individual legacy `PCPT` entry.
fn parse_pcpt_entry(entry: &[u8]) -> Option<AnlzCue> {
    if entry.len() < 40 {
        return None;
    }

    let hot_cue_raw = u32::from_be_bytes([entry[12], entry[13], entry[14], entry[15]]);
    let hot_cue = match hot_cue_raw {
        0 => None,
        1..=8 => Some(hot_cue_raw as u8),
        other => u8::try_from(other).ok(),
    };

    let cue_type = entry[28];
    let is_loop = cue_type == 2;

    let time_ms = u32::from_be_bytes([entry[32], entry[33], entry[34], entry[35]]);
    let loop_time_raw = u32::from_be_bytes([entry[36], entry[37], entry[38], entry[39]]);
    let loop_time_ms = if is_loop && loop_time_raw != u32::MAX {
        Some(loop_time_raw)
    } else {
        None
    };

    Some(AnlzCue {
        hot_cue,
        is_loop,
        time_ms,
        loop_time_ms,
        comment: None,
        color_code: None,
        color_rgb: None,
    })
}

/// Extracts all cues from the sections of an ANLZ file (`.EXT` or `.DAT`).
///
/// If any `PCO2` (extended cue) sections are found, their cues are returned.
/// Multiple `PCO2` sections (e.g. one for hot cues and one for memory cues)
/// are combined.
/// If no `PCO2` sections are present, any `PCOB` sections are returned as fallback.
pub fn parse_anlz_cues(bytes: &[u8]) -> Vec<AnlzCue> {
    if bytes.len() < 12 {
        return Vec::new();
    }

    let file_header_size = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
    let total_size = (u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize)
        .min(bytes.len());

    let mut position = file_header_size;
    let mut pco2_cues = Vec::new();
    let mut pcob_cues = Vec::new();

    while position + 12 <= total_size {
        let remaining = &bytes[position..total_size];
        let tag = &remaining[..4];
        let sec_total_size = u32::from_be_bytes([
            remaining[8],
            remaining[9],
            remaining[10],
            remaining[11],
        ]) as usize;

        if sec_total_size < 12 || sec_total_size > remaining.len() {
            break;
        }

        let section_bytes = &remaining[..sec_total_size];
        if tag == PCO2_MAGIC {
            pco2_cues.extend(parse_pco2(section_bytes));
        } else if tag == PCOB_MAGIC {
            pcob_cues.extend(parse_pcob(section_bytes));
        }

        position += sec_total_size;
    }

    if !pco2_cues.is_empty() {
        pco2_cues
    } else {
        pcob_cues
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("valid hex"))
            .collect()
    }

    #[test]
    fn parses_pcp2_hot_cue_with_rgb_color() {
        // Real bytes extracted from 1098.EXT hot cue 1 (pad 7)
        let hex = concat!(
            "504350320000001000000058", // PCP2, hsize=16, len=88
            "00000007",                 // hot_cue = 7 (G)
            "01",                       // cue_type = 1 (point)
            "0003e8",                   // unknown / 1000
            "00043020",                 // time = 274464 ms
            "ffffffff",                 // loop_time = None
            "000100000000000000000000", // color_id, loop fractions...
            "00000000",                 // len_comment = 0
            "16",                       // color_code = 0x16 (22)
            "1aff00",                   // rgb = (26, 255, 0)
            "00000000000000000000000000000000000000000000000000000000000000000000000000000000"
        );
        let bytes = decode_hex(hex);
        let cue = parse_pcp2_entry(&bytes).expect("cue parsed");

        assert_eq!(cue.hot_cue, Some(7));
        assert!(!cue.is_loop);
        assert_eq!(cue.time_ms, 274464);
        assert_eq!(cue.loop_time_ms, None);
        assert_eq!(cue.color_code, Some(0x16));
        assert_eq!(cue.color_rgb, Some((26, 255, 0)));
        assert_eq!(cue.comment, None);
    }

    #[test]
    fn parses_pcp2_loop_cue() {
        // Real bytes extracted from 2389.EXT loop cue (pad 6)
        let hex = concat!(
            "504350320000001000000058",
            "00000006",                 // hot_cue = 6
            "02",                       // cue_type = 2 (loop)
            "0003e8",
            "0000fe78",                 // time = 65144 ms
            "00010532",                 // loop_time = 66866 ms
            "000100000000000000040001",
            "00000000",                 // len_comment = 0
            "00",                       // color_code = 0
            "ff8c00",                   // rgb = (255, 140, 0)
            "00000000000000000000000000000000000000000000000000000000000000000000000000000000"
        );
        let bytes = decode_hex(hex);
        let cue = parse_pcp2_entry(&bytes).expect("loop cue parsed");

        assert_eq!(cue.hot_cue, Some(6));
        assert!(cue.is_loop);
        assert_eq!(cue.time_ms, 65144);
        assert_eq!(cue.loop_time_ms, Some(66866));
        assert_eq!(cue.color_rgb, Some((255, 140, 0)));
    }

    #[test]
    fn parses_pcpt_legacy_cue() {
        let hex = concat!(
            "504350540000001c00000038", // PCPT, hsize=28, len=56
            "00000003",                 // hot_cue = 3
            "0000000000010000ffffffff",
            "01",                       // cue_type = 1
            "0003e8",
            "0002cc50",                 // time = 183376 ms
            "ffffffff",                 // loop_time = None
            "00000000000000000000000000000000"
        );
        let bytes = decode_hex(hex);
        let cue = parse_pcpt_entry(&bytes).expect("pcpt parsed");

        assert_eq!(cue.hot_cue, Some(3));
        assert!(!cue.is_loop);
        assert_eq!(cue.time_ms, 183376);
        assert_eq!(cue.loop_time_ms, None);
    }
}
