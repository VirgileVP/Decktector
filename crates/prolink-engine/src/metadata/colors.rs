//! Cue color lookup tables — used only by the NFS route's cue mapping
//! ([`super::nfs`]), since [`prolink::dbserver::cues`] stays stubbed (see
//! its own doc comment) and never produces a color to map.

use rekordcrate::util::ColorIndex;

/// Rekordbox's hot-cue color-code palette, verbatim from `rekordcrate`'s
/// own `ExtendedCue::hot_cue_color_index` doc comment (`anlz.rs`) — a
/// direct read of the actual dependency's source, not a secondary
/// paraphrase, unlike some of this phase's other tables. `0x00` means "no
/// color" (`None` here); this function only covers `0x01..=0x3e`.
pub fn hot_cue_color_hex(index: u8) -> Option<&'static str> {
    let hex = match index {
        0x01 => "#305aff",
        0x02 => "#5073ff",
        0x03 => "#508cff",
        0x04 => "#50a0ff",
        0x05 => "#50b4ff",
        0x06 => "#50b0f2",
        0x07 => "#50aee8",
        0x08 => "#45acdb",
        0x09 => "#00e0ff",
        0x0a => "#19daf0",
        0x0b => "#32d2e6",
        0x0c => "#21b4b9",
        0x0d => "#20aaa0",
        0x0e => "#1fa392",
        0x0f => "#19a08c",
        0x10 => "#14a584",
        0x11 => "#14aa7d",
        0x12 => "#10b176",
        0x13 => "#30d26e",
        0x14 => "#37de5a",
        0x15 => "#3ceb50",
        0x16 => "#28e214",
        0x17 => "#7dc13d",
        0x18 => "#8cc832",
        0x19 => "#9bd723",
        0x1a => "#a5e116",
        0x1b => "#a5dc0a",
        0x1c => "#aad208",
        0x1d => "#b4c805",
        0x1e => "#b4be04",
        0x1f => "#bab404",
        0x20 => "#c3af04",
        0x21 => "#e1aa00",
        0x22 => "#ffa000",
        0x23 => "#ff9600",
        0x24 => "#ff8c00",
        0x25 => "#ff7500",
        0x26 => "#e0641b",
        0x27 => "#e0461e",
        0x28 => "#e0301e",
        0x29 => "#e02823",
        0x2a => "#e62828",
        0x2b => "#ff376f",
        0x2c => "#ff2d6f",
        0x2d => "#ff127b",
        0x2e => "#f51e8c",
        0x2f => "#eb2da0",
        0x30 => "#e637b4",
        0x31 => "#de44cf",
        0x32 => "#de448d",
        0x33 => "#e630b4",
        0x34 => "#e619dc",
        0x35 => "#e600ff",
        0x36 => "#dc00ff",
        0x37 => "#cc00ff",
        0x38 => "#b432ff",
        0x39 => "#b93cff",
        0x3a => "#c542ff",
        0x3b => "#aa5aff",
        0x3c => "#aa72ff",
        0x3d => "#8272ff",
        0x3e => "#6473ff",
        _ => return None,
    };
    Some(hex)
}

/// Memory-cue color palette (`rekordcrate::util::ColorIndex`) — a distinct,
/// smaller palette from hot cues' 62-entry code table.
pub fn memory_cue_color_hex(color: ColorIndex) -> Option<&'static str> {
    match color {
        ColorIndex::None => None,
        ColorIndex::Pink => Some("#ff2d92"),
        ColorIndex::Red => Some("#ff453a"),
        ColorIndex::Orange => Some("#ff9500"),
        ColorIndex::Yellow => Some("#ffd60a"),
        ColorIndex::Green => Some("#00c853"),
        ColorIndex::Aqua => Some("#00d4ff"),
        ColorIndex::Blue => Some("#0a84ff"),
        ColorIndex::Purple => Some("#9d5cff"),
    }
}

/// Per-kind fallback when neither of the above gives a color — ported
/// directly from decktective's pre-Rust engine's `cues.ts`.
pub const DEFAULT_MEMORY_COLOR: &str = "#ff9f2e";
pub const DEFAULT_HOTCUE_COLOR: &str = "#12d17e";
pub const DEFAULT_LOOP_COLOR: &str = "#ff8a00";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_known_hot_cue_color_resolves() {
        assert_eq!(hot_cue_color_hex(0x01), Some("#305aff"));
        assert_eq!(hot_cue_color_hex(0x3e), Some("#6473ff"));
    }

    #[test]
    fn zero_and_out_of_range_hot_cue_codes_have_no_color() {
        assert_eq!(hot_cue_color_hex(0x00), None);
        assert_eq!(hot_cue_color_hex(0x3f), None);
    }

    #[test]
    fn color_index_none_has_no_memory_cue_color() {
        assert_eq!(memory_cue_color_hex(ColorIndex::None), None);
    }

    #[test]
    fn a_named_color_index_resolves() {
        assert_eq!(memory_cue_color_hex(ColorIndex::Blue), Some("#0a84ff"));
    }
}
