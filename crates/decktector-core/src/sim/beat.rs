//! Beat arithmetic over a track's beat grid, with BPM as the fallback.
//!
//! Loops snap to beats, sync aligns beat phase and the deck display counts
//! beats within a bar; all three need the same answer to "which beat is the
//! playhead in?". [`BeatMap`] gives it, from the recorded grid where the
//! track has one and from a metronome built on the track's BPM where it does
//! not, or where the playhead is outside the grid.

use crate::model::{BeatGridBeat, TrackInfo};

pub(super) const BEATS_PER_BAR: u8 = 4;

/// Slack when comparing a position with a grid offset, so a playhead sitting
/// exactly on a beat is not judged to be a rounding error before it.
const BEAT_EPSILON_MS: f64 = 1e-6;
const MS_PER_MINUTE: f64 = 60_000.0;

/// One beat: where it starts, how long it lasts, and its place in the bar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Beat {
    pub start_ms: f64,
    pub length_ms: f64,
    pub beat_in_measure: u8,
}

impl Beat {
    /// How far into the beat `position_ms` is, from 0 up to (excluding) 1.
    pub fn phase_at(&self, position_ms: f64) -> f64 {
        ((position_ms - self.start_ms) / self.length_ms).clamp(0.0, 1.0 - f64::EPSILON)
    }
}

/// Beat lookup for one track.
pub(super) struct BeatMap<'track> {
    grid: &'track [BeatGridBeat],
    /// Length of a beat where the grid has nothing to say, if the track has
    /// a BPM or a grid with at least two beats to measure one from.
    fallback_length_ms: Option<f64>,
}

impl<'track> BeatMap<'track> {
    pub fn of(track: &'track TrackInfo) -> Self {
        let grid = track.beat_grid.as_deref().unwrap_or(&[]);
        let fallback_length_ms = track
            .bpm
            .filter(|bpm| *bpm > 0.0)
            .map(|bpm| MS_PER_MINUTE / bpm)
            .or(match grid {
                [.., before_last, last] => Some(last.offset_ms - before_last.offset_ms),
                _ => None,
            })
            .filter(|length| *length > 0.0);
        Self {
            grid,
            fallback_length_ms,
        }
    }

    /// The beat `position_ms` falls in, or `None` when the track has neither
    /// a usable grid nor a BPM.
    pub fn beat_at(&self, position_ms: f64) -> Option<Beat> {
        let passed = self
            .grid
            .partition_point(|beat| beat.offset_ms <= position_ms + BEAT_EPSILON_MS);

        match (passed, self.grid.len()) {
            (0, 0) => self.extrapolate(0.0, 1, position_ms),
            (0, _) => {
                let first = self.grid[0];
                self.extrapolate(first.offset_ms, first.beat_in_measure, position_ms)
            }
            (passed, len) if passed == len => {
                let last = self.grid[len - 1];
                self.extrapolate(last.offset_ms, last.beat_in_measure, position_ms)
            }
            (passed, _) => {
                let beat = self.grid[passed - 1];
                Some(Beat {
                    start_ms: beat.offset_ms,
                    length_ms: self.grid[passed].offset_ms - beat.offset_ms,
                    beat_in_measure: beat.beat_in_measure,
                })
            }
        }
    }

    /// The start of the beat `count` beats after the one starting at
    /// `start_ms`.
    pub fn beat_start_after(&self, start_ms: f64, count: u32) -> Option<f64> {
        let mut beat = self.beat_at(start_ms)?;
        for _ in 0..count {
            beat = self.beat_at(beat.start_ms + beat.length_ms)?;
        }
        Some(beat.start_ms)
    }

    /// Continues the metronome from a known beat, forwards or backwards.
    fn extrapolate(&self, anchor_ms: f64, anchor_beat: u8, position_ms: f64) -> Option<Beat> {
        let length_ms = self.fallback_length_ms?;
        let beats_from_anchor = ((position_ms - anchor_ms + BEAT_EPSILON_MS) / length_ms).floor();
        let beat_in_measure = (i64::from(anchor_beat) - 1 + beats_from_anchor as i64)
            .rem_euclid(i64::from(BEATS_PER_BAR)) as u8
            + 1;
        Some(Beat {
            start_ms: anchor_ms + beats_from_anchor * length_ms,
            length_ms,
            beat_in_measure,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::test_support::{track_with_grid, track_without_grid};

    #[test]
    fn a_position_inside_the_grid_belongs_to_the_beat_it_follows() {
        let track = track_with_grid(120.0, 16);
        let beat = BeatMap::of(&track).beat_at(1_250.0).expect("a beat");

        assert_eq!(beat.start_ms, 1_000.0);
        assert_eq!(beat.length_ms, 500.0);
        assert_eq!(beat.beat_in_measure, 3);
        assert_eq!(beat.phase_at(1_250.0), 0.5);
    }

    #[test]
    fn a_position_exactly_on_a_beat_belongs_to_that_beat() {
        let track = track_with_grid(120.0, 16);

        assert_eq!(
            BeatMap::of(&track).beat_at(1_500.0).unwrap().start_ms,
            1_500.0
        );
    }

    #[test]
    fn the_metronome_continues_past_the_end_of_the_grid() {
        let track = track_with_grid(120.0, 4);
        let beat = BeatMap::of(&track).beat_at(2_600.0).expect("a beat");

        assert_eq!(beat.start_ms, 2_500.0);
        assert_eq!(beat.beat_in_measure, 2);
    }

    #[test]
    fn a_track_without_a_grid_falls_back_to_its_bpm() {
        let track = track_without_grid(120.0);
        let beat = BeatMap::of(&track).beat_at(1_200.0).expect("a beat");

        assert_eq!(beat.start_ms, 1_000.0);
        assert_eq!(beat.beat_in_measure, 3);
    }

    #[test]
    fn a_track_with_neither_grid_nor_bpm_has_no_beats() {
        let mut track = track_without_grid(120.0);
        track.bpm = None;

        assert_eq!(BeatMap::of(&track).beat_at(1_000.0), None);
    }

    #[test]
    fn counting_beats_follows_the_grid() {
        let track = track_with_grid(120.0, 32);

        assert_eq!(
            BeatMap::of(&track).beat_start_after(500.0, 4),
            Some(2_500.0)
        );
    }
}
