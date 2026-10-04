//! Maps dbserver responses onto `decktector_core::model::TrackInfo` fields.

use decktector_core::model::{BeatGridBeat, WaveformData, WaveformSegment};
use prolink::dbserver::beatgrid::DbBeatGridEntry;
use prolink::dbserver::client::TrackMenuItem;
use prolink::dbserver::queries::MenuItemType;
use prolink::dbserver::waveform;

/// BPM stored as-is if plausible, `/100` if it looks like the protocol's
/// occasional "BPM x100" encoding — ported from decktector's pre-Rust
/// engine's `normalizeTempo` (`track-loader.ts`), corroborated by
/// `beat-link`'s own `TEMPO` menu-item-type doc comment ("the tempo of a
/// track, BPM times 100"). `pub(crate)`: also reused by `metadata::nfs`,
/// whose PDB-sourced tempo field carries the same encoding ambiguity.
pub(crate) fn normalize_tempo(raw: f64) -> Option<f64> {
    if raw <= 0.0 {
        return None;
    }
    Some(if raw > 400.0 { raw / 100.0 } else { raw })
}

fn text_field(items: &[TrackMenuItem], item_type: MenuItemType) -> Option<String> {
    items
        .iter()
        .find(|item| item.item_type == item_type)
        .and_then(|item| item.text.clone())
}

fn numeric_field(items: &[TrackMenuItem], item_type: MenuItemType) -> Option<u32> {
    items
        .iter()
        .find(|item| item.item_type == item_type)
        .and_then(|item| item.number)
}

pub fn title(items: &[TrackMenuItem]) -> Option<String> {
    text_field(items, MenuItemType::TrackTitle)
}

pub fn artist(items: &[TrackMenuItem]) -> Option<String> {
    text_field(items, MenuItemType::Artist)
}

pub fn album(items: &[TrackMenuItem]) -> Option<String> {
    text_field(items, MenuItemType::AlbumTitle)
}

pub fn genre(items: &[TrackMenuItem]) -> Option<String> {
    text_field(items, MenuItemType::Genre)
}

pub fn key(items: &[TrackMenuItem]) -> Option<String> {
    text_field(items, MenuItemType::Key)
}

pub fn label(items: &[TrackMenuItem]) -> Option<String> {
    text_field(items, MenuItemType::Label)
}

pub fn comment(items: &[TrackMenuItem]) -> Option<String> {
    text_field(items, MenuItemType::Comment)
}

pub fn duration_ms(items: &[TrackMenuItem]) -> f64 {
    numeric_field(items, MenuItemType::Duration)
        .map(|seconds| f64::from(seconds) * 1000.0)
        .unwrap_or(0.0)
}

/// Prefers the track's own tempo; falls back to `status_bpm` (the status
/// packet's live BPM reading) only when the metadata response doesn't have
/// one — matches `track-loader.ts`'s `normalizeTempo(track.tempo) ??
/// statusBpm`.
pub fn bpm(items: &[TrackMenuItem], status_bpm: Option<f64>) -> Option<f64> {
    numeric_field(items, MenuItemType::Tempo)
        .and_then(|raw| normalize_tempo(f64::from(raw)))
        .or(status_bpm)
}

pub fn beat_grid(entries: &[DbBeatGridEntry]) -> Vec<BeatGridBeat> {
    entries
        .iter()
        .map(|entry| BeatGridBeat {
            offset_ms: f64::from(entry.time_ms),
            beat_in_measure: entry.beat_in_bar,
        })
        .collect()
}

pub fn waveform_data(
    columns: &[prolink::dbserver::waveform::DbWaveformColumn],
) -> Option<WaveformData> {
    if columns.is_empty() {
        return None;
    }
    let detail: Vec<WaveformSegment> = columns
        .iter()
        .map(|column| WaveformSegment {
            height: column.height,
            color: column.color,
        })
        .collect();
    let preview = decktector_core::waveform::build_preview_from_detail(
        &detail,
        decktector_core::waveform::PREVIEW_SEGMENT_COUNT,
    );
    Some(WaveformData {
        detail: Some(detail),
        ms_per_detail_segment: waveform::ms_per_segment(),
        preview: Some(preview),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(item_type: MenuItemType, text: Option<&str>, number: Option<u32>) -> TrackMenuItem {
        TrackMenuItem {
            item_type,
            text: text.map(str::to_string),
            number,
        }
    }

    #[test]
    fn extracts_the_title() {
        let items = vec![item(MenuItemType::TrackTitle, Some("Strobe"), None)];
        assert_eq!(title(&items), Some("Strobe".to_string()));
    }

    #[test]
    fn normal_tempo_values_pass_through() {
        let items = vec![item(MenuItemType::Tempo, None, Some(128))];
        assert_eq!(bpm(&items, None), Some(128.0));
    }

    #[test]
    fn tempo_stored_times_100_is_divided_back_down() {
        let items = vec![item(MenuItemType::Tempo, None, Some(12_800))];
        assert_eq!(bpm(&items, None), Some(128.0));
    }

    #[test]
    fn missing_tempo_falls_back_to_the_status_packet_bpm() {
        assert_eq!(bpm(&[], Some(174.0)), Some(174.0));
    }

    #[test]
    fn duration_converts_seconds_to_milliseconds() {
        let items = vec![item(MenuItemType::Duration, None, Some(245))];
        assert_eq!(duration_ms(&items), 245_000.0);
    }

    #[test]
    fn beat_grid_maps_time_and_bar_position() {
        let entries = vec![DbBeatGridEntry {
            beat_in_bar: 1,
            tempo_centibpm: 12_800,
            time_ms: 469,
        }];
        let grid = beat_grid(&entries);
        assert_eq!(grid[0].offset_ms, 469.0);
        assert_eq!(grid[0].beat_in_measure, 1);
    }

    #[test]
    fn an_empty_waveform_response_is_none() {
        assert!(waveform_data(&[]).is_none());
    }
}
