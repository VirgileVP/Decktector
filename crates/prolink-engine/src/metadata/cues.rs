//! Maps cues as a player's analysis records them ([`AnlzCue`]) onto the
//! model's [`TrackCue`] — the one mapping both metadata routes share: the
//! NFS route reads them out of a track's analysis files or its
//! `exportLibrary.db`, the dbserver route out of a player's cue list reply.

use decktector_core::model::{CueKind, TrackCue};
use prolink::nfs::AnlzCue;

use super::colors;

/// Every cue, mapped, in playing order.
pub fn track_cues(cues: &[AnlzCue]) -> Vec<TrackCue> {
    let mut mapped: Vec<TrackCue> = cues.iter().map(track_cue).collect();
    mapped.sort_by(|a, b| a.offset_ms.total_cmp(&b.offset_ms));
    mapped
}

pub fn track_cue(cue: &AnlzCue) -> TrackCue {
    let kind = match (cue.hot_cue.is_some(), cue.is_loop) {
        (true, true) => CueKind::Hotloop,
        (true, false) => CueKind::Hotcue,
        (false, true) => CueKind::Loop,
        (false, false) => CueKind::Memory,
    };

    TrackCue {
        kind,
        offset_ms: f64::from(cue.time_ms),
        length_ms: cue
            .loop_time_ms
            .map(|loop_time| f64::from(loop_time.saturating_sub(cue.time_ms))),
        button: cue
            .hot_cue
            .map(|number| hot_cue_button_letter(u32::from(number))),
        label: cue.comment.clone(),
        color: marker_color(cue, kind),
    }
}

/// The cue's own RGB when it has one, then its palette color, then the
/// default for its kind.
fn marker_color(cue: &AnlzCue, kind: CueKind) -> String {
    if let Some((r, g, b)) = cue.color_rgb {
        return format!("#{r:02x}{g:02x}{b:02x}");
    }
    let palette_color = cue.color_code.and_then(|code| {
        if cue.hot_cue.is_some() {
            colors::hot_cue_color_hex(code)
        } else {
            memory_cue_palette_hex(code)
        }
    });
    palette_color
        .unwrap_or_else(|| default_color(kind))
        .to_string()
}

/// A memory cue's color, by its index in rekordbox's eight-color palette.
fn memory_cue_palette_hex(code: u8) -> Option<&'static str> {
    match code {
        1 => Some("#ff2d92"), // Pink
        2 => Some("#ff453a"), // Red
        3 => Some("#ff9500"), // Orange
        4 => Some("#ffd60a"), // Yellow
        5 => Some("#00c853"), // Green
        6 => Some("#00d4ff"), // Aqua
        7 => Some("#0a84ff"), // Blue
        8 => Some("#9d5cff"), // Purple
        _ => None,
    }
}

pub fn default_color(kind: CueKind) -> &'static str {
    match kind {
        CueKind::Memory => colors::DEFAULT_MEMORY_COLOR,
        CueKind::Hotcue => colors::DEFAULT_HOTCUE_COLOR,
        CueKind::Loop | CueKind::Hotloop => colors::DEFAULT_LOOP_COLOR,
    }
}

/// `1` -> `"A"`, `8` -> `"H"` — the letters real hardware labels hot-cue
/// pads with; anything outside that range (shouldn't happen for an actual
/// hot cue) falls back to the raw number rather than guessing at a letter.
pub fn hot_cue_button_letter(hot_cue: u32) -> String {
    match u8::try_from(hot_cue) {
        Ok(number @ 1..=8) => char::from(b'A' + number - 1).to_string(),
        _ => hot_cue.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hot_cue_at(hot_cue: Option<u8>, time_ms: u32) -> AnlzCue {
        AnlzCue {
            hot_cue,
            is_loop: false,
            time_ms,
            loop_time_ms: None,
            comment: None,
            color_code: None,
            color_rgb: None,
        }
    }

    #[test]
    fn hot_cue_numbers_one_through_eight_map_to_letters() {
        assert_eq!(hot_cue_button_letter(1), "A");
        assert_eq!(hot_cue_button_letter(8), "H");
    }

    #[test]
    fn an_out_of_range_hot_cue_number_falls_back_to_the_digit() {
        assert_eq!(hot_cue_button_letter(0), "0");
        assert_eq!(hot_cue_button_letter(99), "99");
    }

    #[test]
    fn maps_a_hot_cue_with_rgb() {
        let cue = AnlzCue {
            comment: Some("Drop 1".to_string()),
            color_code: Some(0x16),
            color_rgb: Some((255, 0, 128)),
            ..hot_cue_at(Some(1), 12345)
        };
        let track_cue = track_cue(&cue);
        assert_eq!(track_cue.kind, CueKind::Hotcue);
        assert_eq!(track_cue.offset_ms, 12345.0);
        assert_eq!(track_cue.length_ms, None);
        assert_eq!(track_cue.button, Some("A".to_string()));
        assert_eq!(track_cue.label, Some("Drop 1".to_string()));
        assert_eq!(track_cue.color, "#ff0080");
    }

    #[test]
    fn maps_a_memory_loop() {
        let cue = AnlzCue {
            is_loop: true,
            loop_time_ms: Some(9000),
            color_code: Some(5), // Green in memory cue palette
            ..hot_cue_at(None, 5000)
        };
        let track_cue = track_cue(&cue);
        assert_eq!(track_cue.kind, CueKind::Loop);
        assert_eq!(track_cue.offset_ms, 5000.0);
        assert_eq!(track_cue.length_ms, Some(4000.0));
        assert_eq!(track_cue.button, None);
        assert_eq!(track_cue.label, None);
        assert_eq!(track_cue.color, "#00c853");
    }

    #[test]
    fn a_cue_without_color_takes_its_kinds_default() {
        let track_cue = track_cue(&hot_cue_at(Some(2), 1000));
        assert_eq!(track_cue.color, colors::DEFAULT_HOTCUE_COLOR);
    }

    #[test]
    fn cues_come_out_in_playing_order() {
        let cues = track_cues(&[hot_cue_at(Some(2), 50_000), hot_cue_at(Some(1), 10_000)]);
        let offsets: Vec<f64> = cues.iter().map(|cue| cue.offset_ms).collect();
        assert_eq!(offsets, vec![10_000.0, 50_000.0]);
    }
}
