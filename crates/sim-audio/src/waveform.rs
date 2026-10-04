//! Columns to the [`WaveformData`] the UI already draws.

use decktector_core::model::{WaveformData, WaveformSegment};
use decktector_core::waveform::{build_preview_from_detail, PREVIEW_SEGMENT_COUNT};

use crate::columns::{Column, DETAIL_COLUMNS_PER_SECOND};

/// Reference colours the three bands blend between. A player's HD waveform
/// reads the same way: bass blue, body amber, air near-white.
const LOW_COLOR: [f32; 3] = [40.0, 90.0, 255.0];
const MID_COLOR: [f32; 3] = [255.0, 160.0, 60.0];
const HIGH_COLOR: [f32; 3] = [235.0, 245.0, 255.0];
/// Loudness percentile taken as full height, so one clipped sample cannot
/// flatten the whole track against the top of the scale. Used for the
/// column's height and, separately, for its brightness.
const PEAK_PERCENTILE: f64 = 0.995;
/// Floor under the colour of a quiet column, so a breakdown reads as dim
/// rather than as a hole in the waveform.
const MIN_BRIGHTNESS: f32 = 0.35;
/// How hard the band a column leans on wins over the other two. Blending
/// the three by their raw share averages every column toward grey, because
/// music always holds some of all three; squaring the shares first lets the
/// dominant band carry the colour the way it does on a player.
const COLOR_CONTRAST: f32 = 2.0;

/// Builds the detail and preview waveforms.
///
/// Returns `None` for a track with no columns at all — an empty or
/// unreadable file — rather than an empty waveform, which the UI would
/// draw as an analysed track that happens to be silent.
pub fn build(columns: &[Column]) -> Option<WaveformData> {
    if columns.is_empty() {
        return None;
    }

    let full_scale = percentile_peak(columns, PEAK_PERCENTILE);
    let full_energy = percentile(
        columns
            .iter()
            .map(|column| column.bands.iter().copied().fold(0.0, f32::max)),
        PEAK_PERCENTILE,
    );

    let detail: Vec<WaveformSegment> = columns
        .iter()
        .map(|column| WaveformSegment {
            height: (column.peak / full_scale).clamp(0.0, 1.0),
            color: color_of(column, full_energy),
        })
        .collect();

    Some(WaveformData {
        preview: Some(build_preview_from_detail(&detail, PREVIEW_SEGMENT_COUNT)),
        ms_per_detail_segment: 1000.0 / DETAIL_COLUMNS_PER_SECOND,
        detail: Some(detail),
    })
}

fn percentile_peak(columns: &[Column], percentile_of: f64) -> f32 {
    percentile(columns.iter().map(|column| column.peak), percentile_of)
}

/// The value at `percentile_of` through `values`, never zero — every use
/// here divides by it.
fn percentile(values: impl Iterator<Item = f32>, percentile_of: f64) -> f32 {
    let mut sorted: Vec<f32> = values.collect();
    if sorted.is_empty() {
        return f32::EPSILON;
    }
    sorted.sort_by(f32::total_cmp);
    let index = ((sorted.len() - 1) as f64 * percentile_of).round() as usize;
    sorted[index].max(f32::EPSILON)
}

/// Blends the band reference colours by which band the column leans on,
/// then dims the result by how loud the column is against the body of the
/// track.
fn color_of(column: &Column, full_energy: f32) -> [u8; 3] {
    let weights = column.bands.map(|band| band.powf(COLOR_CONTRAST));
    let total: f32 = weights.iter().sum();
    if total <= 0.0 {
        return [0, 0, 0];
    }

    let loudest = column.bands.iter().copied().fold(0.0, f32::max);
    // Square root because loudness is perceived closer to that than to the
    // linear amplitude, and a linear ramp leaves most of a track's columns
    // looking flat and dark.
    let brightness =
        MIN_BRIGHTNESS + (1.0 - MIN_BRIGHTNESS) * (loudest / full_energy).clamp(0.0, 1.0).sqrt();

    let mut channels = [0.0f32; 3];
    for (weight, reference) in weights.iter().zip([LOW_COLOR, MID_COLOR, HIGH_COLOR]) {
        for (channel, value) in channels.iter_mut().zip(reference) {
            *channel += (weight / total) * value;
        }
    }

    channels.map(|channel| (channel * brightness).round().clamp(0.0, 255.0) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn column(peak: f32, bands: [f32; 3]) -> Column {
        Column { peak, bands }
    }

    #[test]
    fn a_track_with_no_columns_has_no_waveform() {
        assert!(build(&[]).is_none());
    }

    #[test]
    fn the_preview_holds_the_column_count_the_ui_expects() {
        let columns = vec![column(0.5, [0.2, 0.1, 0.05]); 5_000];

        let waveform = build(&columns).unwrap();

        assert_eq!(waveform.preview.unwrap().len(), PREVIEW_SEGMENT_COUNT);
        assert_eq!(waveform.detail.unwrap().len(), 5_000);
        assert!((waveform.ms_per_detail_segment - 1000.0 / 150.0).abs() < f64::EPSILON);
    }

    #[test]
    fn heights_scale_against_the_loud_part_of_the_track() {
        // A quiet intro and outro around a loud body, the shape every DJ
        // track has: full scale comes from the body, not from the intro.
        let mut columns = vec![column(0.1, [0.1, 0.0, 0.0]); 1_000];
        columns[300..900].fill(column(0.5, [0.5, 0.0, 0.0]));

        let detail = build(&columns).unwrap().detail.unwrap();

        assert!(detail[500].height > 0.99, "{}", detail[500].height);
        assert!(
            (detail[0].height - 0.2).abs() < 0.01,
            "{}",
            detail[0].height
        );
    }

    #[test]
    fn a_single_clipped_column_does_not_flatten_the_rest() {
        let mut columns = vec![column(0.5, [0.5, 0.0, 0.0]); 1_000];
        columns[10] = column(1.0, [1.0, 0.0, 0.0]);

        let detail = build(&columns).unwrap().detail.unwrap();

        assert!(detail[500].height > 0.99, "{}", detail[500].height);
    }

    #[test]
    fn a_bass_column_reads_blue_and_a_treble_one_does_not() {
        let bass = color_of(&column(1.0, [1.0, 0.0, 0.0]), 1.0);
        let treble = color_of(&column(1.0, [0.0, 0.0, 1.0]), 1.0);

        assert!(bass[2] > bass[0], "{bass:?}");
        assert!(treble[0] > bass[0], "{treble:?} vs {bass:?}");
    }

    #[test]
    fn the_band_a_column_leans_on_carries_its_colour() {
        // Bass ahead of the other two, as a kick column is: the result has
        // to read blue rather than as an even grey blend.
        let leaning = color_of(&column(1.0, [0.6, 0.3, 0.2]), 1.0);

        assert!(leaning[2] > leaning[0] * 2, "{leaning:?}");
    }

    #[test]
    fn a_quiet_column_keeps_its_hue_but_loses_brightness() {
        let loud = color_of(&column(1.0, [1.0, 0.2, 0.1]), 1.0);
        let quiet = color_of(&column(0.1, [0.1, 0.02, 0.01]), 1.0);

        assert!(quiet[2] < loud[2], "{quiet:?} vs {loud:?}");
        assert!(quiet[2] > quiet[0], "{quiet:?}");
    }

    #[test]
    fn silence_is_black_rather_than_a_division_by_zero() {
        assert_eq!(color_of(&column(0.0, [0.0; 3]), 1.0), [0, 0, 0]);
    }
}
