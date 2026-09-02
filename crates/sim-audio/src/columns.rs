//! The one pass over the decoded audio: every downstream step reads its
//! output rather than the samples.
//!
//! A column covers the same slice of time a player's HD waveform column
//! does, which makes the same series usable twice — as the waveform itself
//! and as the onset envelope the tempo detector runs on. At 150 columns per
//! second a column is 6.7 ms, fine enough to place a beat inside a
//! sixteenth note at any tempo a DJ plays.

use crate::bands::BandSplitter;

/// Columns per second, matching the HD waveforms players serve
/// (`prolink_engine::metadata::nfs`) so the fixture and the live route
/// describe a track at the same resolution.
pub const DETAIL_COLUMNS_PER_SECOND: f64 = 150.0;

/// What one column of the track amounts to.
#[derive(Debug, Clone, Copy, Default)]
pub struct Column {
    /// Loudest absolute sample in the column — what the waveform's height
    /// draws, since a peak is what makes a transient visible.
    pub peak: f32,
    /// Per-band RMS energy: low, mid, high. Drives the column's colour and
    /// the onset envelope.
    pub bands: [f32; 3],
}

/// Splits `samples` into columns of [`DETAIL_COLUMNS_PER_SECOND`].
///
/// The band filters run continuously across column boundaries: resetting
/// them per column would ring at every edge and paint a stripe into the
/// waveform.
pub fn analyze(samples: &[f32], sample_rate: u32) -> Vec<Column> {
    let per_column = (sample_rate as f64 / DETAIL_COLUMNS_PER_SECOND)
        .round()
        .max(1.0) as usize;
    let mut splitter = BandSplitter::new(sample_rate);

    samples
        .chunks(per_column)
        .map(|chunk| {
            let mut peak = 0.0f32;
            let mut energy = [0.0f32; 3];
            for &sample in chunk {
                peak = peak.max(sample.abs());
                let bands = splitter.process(sample);
                for (total, band) in energy.iter_mut().zip(bands) {
                    *total += band * band;
                }
            }
            let count = chunk.len().max(1) as f32;
            Column {
                peak,
                bands: energy.map(|total| (total / count).sqrt()),
            }
        })
        .collect()
}

/// The onset envelope the tempo detector reads: half-wave-rectified rise in
/// band energy from one column to the next.
///
/// Only what grows counts. A kick is a jump in low-band energy and a hat a
/// jump in the high band; energy decaying between hits says nothing about
/// where the next one falls, so it is discarded rather than averaged in.
pub fn onset_envelope(columns: &[Column]) -> Vec<f32> {
    // Log compression before differencing, so a quiet intro's onsets weigh
    // in alongside a loud drop's instead of being flattened by them.
    let compress = |value: f32| (1.0 + value * 100.0).ln();

    columns
        .iter()
        .enumerate()
        .map(|(index, column)| {
            if index == 0 {
                return 0.0;
            }
            let previous = &columns[index - 1];
            column
                .bands
                .iter()
                .zip(previous.bands)
                .map(|(current, earlier)| (compress(*current) - compress(earlier)).max(0.0))
                .sum()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_RATE: u32 = 44_100;

    /// A second of silence, then a second of full-scale bass.
    fn silence_then_bass() -> Vec<f32> {
        let step = 2.0 * std::f32::consts::PI * 60.0 / SAMPLE_RATE as f32;
        let mut samples = vec![0.0; SAMPLE_RATE as usize];
        samples.extend((0..SAMPLE_RATE).map(|i| (i as f32 * step).sin()));
        samples
    }

    #[test]
    fn a_column_covers_a_hundred_and_fiftieth_of_a_second() {
        let columns = analyze(&vec![0.0; SAMPLE_RATE as usize], SAMPLE_RATE);

        assert_eq!(columns.len(), DETAIL_COLUMNS_PER_SECOND as usize);
    }

    #[test]
    fn silence_reads_as_no_peak_and_bass_as_low_band_energy() {
        let columns = analyze(&silence_then_bass(), SAMPLE_RATE);
        let silent = columns[10];
        let loud = columns[columns.len() - 10];

        assert_eq!(silent.peak, 0.0);
        assert!(loud.peak > 0.9, "{}", loud.peak);
        assert!(
            loud.bands[0] > loud.bands[1] && loud.bands[0] > loud.bands[2],
            "{:?}",
            loud.bands
        );
    }

    #[test]
    fn the_envelope_spikes_where_energy_rises_and_stays_flat_after() {
        let columns = analyze(&silence_then_bass(), SAMPLE_RATE);
        let envelope = onset_envelope(&columns);
        let onset = DETAIL_COLUMNS_PER_SECOND as usize;

        let rise: f32 = envelope[onset..onset + 5]
            .iter()
            .copied()
            .fold(0.0, f32::max);
        let sustained: f32 = envelope[onset + 40..onset + 60]
            .iter()
            .copied()
            .fold(0.0, f32::max);
        assert!(rise > sustained * 5.0, "rise {rise}, sustained {sustained}");
    }
}
