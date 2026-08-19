//! Bar-level features, derived from data the players already hand us.
//!
//! One feature vector per bar rather than per waveform column: club tracks are
//! built in bars and phrases, so a bar is the smallest unit where "the kick
//! stopped" is a fact about the music rather than about a single transient.

use crate::model::{BeatGridBeat, WaveformData, WaveformSegment};

/// Number of dimensions in a [`FeatureVector`].
pub const FEATURE_COUNT: usize = 4;

/// A bar's features rescaled to 0-1, ready to be compared with another bar's.
pub type FeatureVector = [f32; FEATURE_COUNT];

/// What one bar of the waveform says about the music playing over it.
///
/// Values are raw here — [`normalize`] rescales them against the track's own
/// distribution before anything compares two bars.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BarFeatures {
    pub start_ms: f64,
    pub end_ms: f64,
    /// Mean column height: how loud the bar is.
    pub energy: f32,
    /// How far the bar's colors lean toward the low band. High while the kick
    /// and bass carry the track, low in a break.
    pub low_share: f32,
    /// Broadband brightness: hats, risers, air.
    pub brightness: f32,
    /// Mean absolute height change between columns — percussive versus
    /// sustained, which separates a drum-led groove from a pad-led breakdown.
    pub transient_rate: f32,
}

impl BarFeatures {
    fn raw_vector(&self) -> FeatureVector {
        [
            self.energy,
            self.low_share,
            self.brightness,
            self.transient_rate,
        ]
    }
}

/// Measures every bar the beat grid defines.
///
/// Returns an empty vec when the track has no detail waveform or fewer than
/// two downbeats: there is no bar to measure, and the caller treats that as
/// "not analysable" rather than as a failure.
pub fn bar_features(
    waveform: &WaveformData,
    beat_grid: &[BeatGridBeat],
    duration_ms: f64,
) -> Vec<BarFeatures> {
    let Some(detail) = waveform.detail.as_deref() else {
        return Vec::new();
    };
    let ms_per_segment = waveform.ms_per_detail_segment;
    if detail.is_empty() || ms_per_segment <= 0.0 {
        return Vec::new();
    }

    let downbeats: Vec<f64> = beat_grid
        .iter()
        .filter(|beat| beat.beat_in_measure == 1)
        .map(|beat| beat.offset_ms)
        .collect();
    if downbeats.len() < 2 {
        return Vec::new();
    }

    downbeats
        .iter()
        .enumerate()
        .filter_map(|(index, &start_ms)| {
            let end_ms = downbeats.get(index + 1).copied().unwrap_or(duration_ms);
            let columns = columns_between(detail, ms_per_segment, start_ms, end_ms)?;
            Some(measure_bar(columns, start_ms, end_ms))
        })
        .collect()
}

/// Median bar energy — the reference every energy threshold is stated against.
pub fn median_energy(bars: &[BarFeatures]) -> f32 {
    let mut energies: Vec<f32> = bars.iter().map(|bar| bar.energy).collect();
    energies.sort_by(f32::total_cmp);
    percentile(&energies, 0.5)
}

/// Rescales every feature against the track's own 5th-95th percentile, so a
/// threshold means the same thing on a quietly mastered track as on a loud one.
pub fn normalize(bars: &[BarFeatures]) -> Vec<FeatureVector> {
    let raw: Vec<FeatureVector> = bars.iter().map(BarFeatures::raw_vector).collect();
    let mut scaled = raw.clone();

    for dimension in 0..FEATURE_COUNT {
        let mut values: Vec<f32> = raw.iter().map(|vector| vector[dimension]).collect();
        values.sort_by(f32::total_cmp);
        let low = percentile(&values, 0.05);
        let span = percentile(&values, 0.95) - low;

        for vector in &mut scaled {
            // A flat dimension carries no information: pin it rather than
            // amplify rounding noise into a fake signal.
            vector[dimension] = if span > f32::EPSILON {
                ((vector[dimension] - low) / span).clamp(0.0, 1.0)
            } else {
                0.0
            };
        }
    }

    scaled
}

/// Value at `quantile` (0-1) of an ascending slice.
fn percentile(sorted: &[f32], quantile: f32) -> f32 {
    if sorted.is_empty() {
        return 0.0;
    }
    let last = sorted.len() - 1;
    let index = (quantile * last as f32).round() as usize;
    sorted[index.min(last)]
}

fn columns_between(
    detail: &[WaveformSegment],
    ms_per_segment: f64,
    start_ms: f64,
    end_ms: f64,
) -> Option<&[WaveformSegment]> {
    let first = (start_ms.max(0.0) / ms_per_segment).floor() as usize;
    let end = ((end_ms / ms_per_segment).ceil() as usize).min(detail.len());
    (first < end).then(|| &detail[first..end])
}

fn measure_bar(columns: &[WaveformSegment], start_ms: f64, end_ms: f64) -> BarFeatures {
    let count = columns.len() as f32;
    let mut energy = 0.0;
    let mut low_share = 0.0;
    let mut brightness = 0.0;

    for column in columns {
        let (low, bright) = column_bands(column);
        energy += column.height;
        low_share += low;
        brightness += bright;
    }

    let transient_rate = columns
        .windows(2)
        .map(|pair| (pair[1].height - pair[0].height).abs())
        .sum::<f32>()
        / (count - 1.0).max(1.0);

    BarFeatures {
        start_ms,
        end_ms,
        energy: energy / count,
        low_share: low_share / count,
        brightness: brightness / count,
        transient_rate,
    }
}

/// Reads a column's color as a coarse two-band split: how much it leans low,
/// and how broadband it is.
///
/// This is rekordbox's color waveform convention — bass-heavy content paints
/// blue, broadband content paints toward white — and the simulation engine
/// draws to the same convention. It is the one assumption in the whole
/// pipeline that depends on the palette, which is why it lives in a single
/// function: recalibrating means editing these two lines.
///
/// Both readings are relative to the column's own channels, so a track
/// rendered brighter than another still compares fairly. Neither is weighted
/// by height: `energy` already carries loudness, and keeping the dimensions
/// independent is what makes the novelty curve discriminate.
fn column_bands(segment: &WaveformSegment) -> (f32, f32) {
    let [red, green, blue] = segment.color.map(f32::from);
    let peak = red.max(green).max(blue).max(1.0);
    let low_share = ((blue - red.max(green)) / peak).clamp(0.0, 1.0);
    let brightness = red.min(green).min(blue) / 255.0;
    (low_share, brightness)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn waveform(heights: &[f32], ms_per_segment: f64) -> WaveformData {
        WaveformData {
            detail: Some(
                heights
                    .iter()
                    .map(|&height| WaveformSegment {
                        height,
                        color: [120, 140, 255],
                    })
                    .collect(),
            ),
            mono_detail: None,
            ms_per_detail_segment: ms_per_segment,
            preview: None,
        }
    }

    fn grid(bar_count: usize, bar_ms: f64) -> Vec<BeatGridBeat> {
        (0..bar_count * 4)
            .map(|index| BeatGridBeat {
                offset_ms: index as f64 * bar_ms / 4.0,
                beat_in_measure: (index % 4) as u8 + 1,
            })
            .collect()
    }

    #[test]
    fn one_feature_row_per_downbeat() {
        // 4 bars of 100 ms, one column per 10 ms.
        let bars = bar_features(&waveform(&[0.5; 40], 10.0), &grid(4, 100.0), 400.0);

        assert_eq!(bars.len(), 4);
        assert_eq!(bars[0].start_ms, 0.0);
        assert_eq!(bars[3].start_ms, 300.0);
    }

    #[test]
    fn energy_follows_the_column_heights() {
        let mut heights = [0.2f32; 40];
        heights[20..].fill(0.9);
        let bars = bar_features(&waveform(&heights, 10.0), &grid(4, 100.0), 400.0);

        assert!(bars[0].energy < bars[3].energy);
    }

    #[test]
    fn a_track_without_a_detail_waveform_yields_no_bars() {
        let mut empty = waveform(&[0.5; 40], 10.0);
        empty.detail = None;

        assert!(bar_features(&empty, &grid(4, 100.0), 400.0).is_empty());
    }

    #[test]
    fn a_grid_with_a_single_downbeat_yields_no_bars() {
        let one_bar = vec![BeatGridBeat {
            offset_ms: 0.0,
            beat_in_measure: 1,
        }];

        assert!(bar_features(&waveform(&[0.5; 40], 10.0), &one_bar, 400.0).is_empty());
    }

    #[test]
    fn normalization_spans_zero_to_one() {
        let mut heights = [0.1f32; 80];
        heights[40..].fill(0.9);
        let bars = bar_features(&waveform(&heights, 10.0), &grid(8, 100.0), 800.0);
        let vectors = normalize(&bars);

        let energies: Vec<f32> = vectors.iter().map(|vector| vector[0]).collect();
        assert_eq!(energies.iter().copied().fold(f32::MAX, f32::min), 0.0);
        assert_eq!(energies.iter().copied().fold(0.0, f32::max), 1.0);
    }

    #[test]
    fn a_flat_dimension_normalizes_to_zero_instead_of_noise() {
        let bars = bar_features(&waveform(&[0.5; 40], 10.0), &grid(4, 100.0), 400.0);
        let vectors = normalize(&bars);

        assert!(vectors.iter().all(|vector| vector[0] == 0.0));
    }

    #[test]
    fn blue_columns_read_as_low_band_and_white_columns_as_bright() {
        let bass = WaveformSegment {
            height: 0.8,
            color: [30, 40, 255],
        };
        let broadband = WaveformSegment {
            height: 0.8,
            color: [240, 245, 255],
        };

        let (bass_low, bass_bright) = column_bands(&bass);
        let (broadband_low, broadband_bright) = column_bands(&broadband);

        assert!(bass_low > broadband_low);
        assert!(broadband_bright > bass_bright);
    }
}
