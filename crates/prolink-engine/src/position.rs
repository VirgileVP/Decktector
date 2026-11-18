//! Estimates each deck's playhead position in milliseconds, and how far
//! that estimate can be trusted.

use std::collections::HashMap;

use decktector_core::model::{BeatGridBeat, DeckId, PlayState, PositionQuality, TrackRef};
use prolink::status::DeviceStatus;

use crate::normalize::play_state;

/// How long a CDJ-3000 precise-position reading stays trusted before this
/// falls back to the beat-count approximation. Precise-position packets
/// arrive roughly every 30ms — this matches decktector's pre-Rust engine's
/// own staleness window for the same packets (`PRECISE_POSITION_STALE_MS`
/// in its `position.ts`), which had real CDJ-3000s to tune it against; a
/// couple of seconds, by contrast, would happily keep showing a frozen
/// position for two full seconds after a player actually stopped sending
/// them (unplugged mid-set, say).
const PRECISE_POSITION_FRESHNESS_MS: f64 = 150.0;

const MS_PER_MINUTE: f64 = 60_000.0;

/// A deck's playhead, and where it came from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PositionEstimate {
    pub position_ms: f64,
    pub quality: PositionQuality,
}

#[derive(Debug, Clone, Copy)]
struct PreciseReading {
    position_ms: f64,
    received_at_ms: f64,
}

/// Where a playing deck's beat-count estimate is pinned: the beat its status
/// reports, and the wall-clock instant that beat began — or, with no beat
/// packet to mark that instant, when a status first reported the beat.
#[derive(Debug, Clone, Copy, PartialEq)]
struct BeatAnchor {
    beat_count: u32,
    began_at_ms: f64,
}

/// A deck's beat grid, and the track it was analysed for.
#[derive(Debug, Clone, PartialEq)]
struct TrackGrid {
    track: TrackRef,
    beats: Vec<BeatGridBeat>,
}

/// Where the beat a status reports starts in the track and how long it lasts
/// before the next one, both at 0% pitch.
#[derive(Debug, Clone, Copy, PartialEq)]
struct BeatSpan {
    start_ms: f64,
    length_ms: f64,
    quality: PositionQuality,
}

/// Estimates each deck's playhead in milliseconds from whatever position
/// signal is currently available.
///
/// A CDJ-3000's precise-position packets are used directly while fresh —
/// the only genuinely absolute position this protocol offers
/// ([`PositionQuality::Precise`]). Every other reading (older hardware, or a
/// CDJ-3000 between precise-position packets) starts from the status's beat
/// counter, placed where the loaded track's beat grid says that beat is
/// ([`PositionQuality::BeatGrid`]): the grid knows about lead-in silence and
/// tempo changes. With no grid for the loaded track — metadata still on its
/// way, or none at all — it falls back to `beat_count * (60_000 / bpm)`,
/// which reads the counter as whole beats elapsed since track-ms 0 at a
/// constant tempo ([`PositionQuality::Estimate`]); given the counter's base
/// below, that runs one beat ahead, on top of any lead-in or tempo drift.
///
/// The counter is 1-based: beat 1 is the grid's first beat, and it reads 0
/// during the lead-in before it. A CDJ-3000 capture (2026-08-23) pins this:
/// its precise-position packets put each counter step on a beat, the step
/// to 1 at the first one, after a counter of 0 from track-ms 0.
///
/// A grid is only ever applied to the track it was analysed for: a deck
/// keeps showing its previous track's metadata until the new one's lands,
/// and that grid placed on the new track would be wrong without saying so.
///
/// That beat count only says which beat is playing, and status packets
/// (~5 Hz) report the same one two or three times. While the deck plays,
/// the beat's start is therefore pinned to the instant the beat began (see
/// [`BeatAnchor`]) and projected forward at the deck's speed, up to the next
/// beat and no further. Re-reading the bare beat start from every status
/// instead dragged the position back to it each time — and the timecode
/// generated from it backwards with it, several frames at a time.
#[derive(Debug, Default)]
pub struct PositionTracker {
    precise: HashMap<DeckId, PreciseReading>,
    beat_anchors: HashMap<DeckId, BeatAnchor>,
    /// When each deck's latest beat packet arrived: the moment a new beat
    /// began, which the next status packet only reports late.
    last_beat_at_ms: HashMap<DeckId, f64>,
    beat_grids: HashMap<DeckId, TrackGrid>,
}

impl PositionTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a fresh CDJ-3000 precise-position reading.
    pub fn record_precise_position(&mut self, deck_id: DeckId, position_ms: u32, now_ms: f64) {
        self.precise.insert(
            deck_id,
            PreciseReading {
                position_ms: f64::from(position_ms),
                received_at_ms: now_ms,
            },
        );
    }

    /// Records that a beat packet just marked the start of a new beat.
    pub fn record_beat(&mut self, deck_id: DeckId, now_ms: f64) {
        self.last_beat_at_ms.insert(deck_id, now_ms);
    }

    /// Drops a deck's tracked state. Call this when its device disconnects,
    /// so a stale precise-position reading never survives to be reused if
    /// the same number is later assigned to a different player.
    pub fn forget(&mut self, deck_id: DeckId) {
        self.precise.remove(&deck_id);
        self.beat_anchors.remove(&deck_id);
        self.last_beat_at_ms.remove(&deck_id);
        self.beat_grids.remove(&deck_id);
    }

    /// Gives `deck_id` the beat grid analysed for `track`. It is used only
    /// while a status names that same track as loaded.
    pub fn set_beat_grid(&mut self, deck_id: DeckId, track: TrackRef, beats: Vec<BeatGridBeat>) {
        self.beat_grids.insert(deck_id, TrackGrid { track, beats });
    }

    /// Drops `deck_id`'s beat grid: its metadata no longer carries one.
    pub fn clear_beat_grid(&mut self, deck_id: DeckId) {
        self.beat_grids.remove(&deck_id);
    }

    /// The track `deck_id`'s beat grid was analysed for, if it has one.
    pub fn beat_grid_track(&self, deck_id: DeckId) -> Option<&TrackRef> {
        self.beat_grids.get(&deck_id).map(|grid| &grid.track)
    }

    /// Best current playhead estimate for a status update, and its quality.
    /// `loaded` is the track the status names. `None` when neither a fresh
    /// precise-position reading nor enough of the status packet (beat
    /// count, BPM) is available to approximate from.
    pub fn estimate(
        &mut self,
        deck_id: DeckId,
        status: &DeviceStatus,
        loaded: Option<&TrackRef>,
        now_ms: f64,
    ) -> Option<PositionEstimate> {
        if let Some(reading) = self.precise.get(&deck_id) {
            if now_ms - reading.received_at_ms <= PRECISE_POSITION_FRESHNESS_MS {
                // The beat count may have moved anywhere meanwhile: the next
                // fallback must not resume from an anchor this old.
                self.beat_anchors.remove(&deck_id);
                return Some(PositionEstimate {
                    position_ms: reading.position_ms,
                    quality: PositionQuality::Precise,
                });
            }
        }
        let beat_count = status.beat_count?;
        let bpm = status.track_bpm.filter(|bpm| *bpm > 0.0)?;
        let span = self.beat_span(deck_id, loaded, beat_count, MS_PER_MINUTE / bpm);
        let speed = 1.0 + status.pitch_percent / 100.0;
        let at = |position_ms| PositionEstimate {
            position_ms,
            quality: span.quality,
        };

        if play_state(status.play_state) != PlayState::Playing || speed <= 0.0 {
            // A stopped deck sits wherever it stopped. Neither the time spent
            // there nor a beat packet from before the stop may count once it
            // plays again.
            self.beat_anchors.remove(&deck_id);
            self.last_beat_at_ms.remove(&deck_id);
            return Some(at(span.start_ms));
        }

        let anchor = self.anchor_beat(deck_id, beat_count, span.length_ms / speed, now_ms);
        let into_beat_ms = ((now_ms - anchor.began_at_ms) * speed).clamp(0.0, span.length_ms);
        Some(at(span.start_ms + into_beat_ms))
    }

    /// Where beat `beat_count` lies: on `deck_id`'s grid when that grid was
    /// analysed for `loaded`, otherwise at a constant `beat_ms` per beat.
    fn beat_span(
        &self,
        deck_id: DeckId,
        loaded: Option<&TrackRef>,
        beat_count: u32,
        beat_ms: f64,
    ) -> BeatSpan {
        self.beat_grids
            .get(&deck_id)
            .filter(|grid| loaded == Some(&grid.track))
            .and_then(|grid| grid_beat_span(&grid.beats, beat_count, beat_ms))
            .unwrap_or(BeatSpan {
                start_ms: f64::from(beat_count) * beat_ms,
                length_ms: beat_ms,
                quality: PositionQuality::Estimate,
            })
    }

    /// The anchor for `beat_count`, pinning a new one when the beat changed.
    ///
    /// A beat that follows straight on from the anchored one is pinned to
    /// the beat packet that marked it, if one arrived since the previous
    /// anchor and less than a beat (`beat_wall_ms`, at the deck's speed)
    /// ago. Anything else — the first beat seen, a jump (hot cue, loop,
    /// seek, new track), a beat whose packet went missing — is pinned to
    /// now, the first status to report it.
    fn anchor_beat(
        &mut self,
        deck_id: DeckId,
        beat_count: u32,
        beat_wall_ms: f64,
        now_ms: f64,
    ) -> BeatAnchor {
        let previous = self.beat_anchors.get(&deck_id).copied();
        if let Some(anchor) = previous.filter(|anchor| anchor.beat_count == beat_count) {
            return anchor;
        }
        let marked_at_ms = previous
            .filter(|anchor| anchor.beat_count.checked_add(1) == Some(beat_count))
            .and_then(|anchor| {
                self.last_beat_at_ms
                    .get(&deck_id)
                    .copied()
                    .filter(|beat_at_ms| {
                        *beat_at_ms > anchor.began_at_ms && now_ms - beat_at_ms < beat_wall_ms
                    })
            });
        let anchor = BeatAnchor {
            beat_count,
            began_at_ms: marked_at_ms.unwrap_or(now_ms),
        };
        self.beat_anchors.insert(deck_id, anchor);
        anchor
    }
}

/// Beat `beat_count` placed on `beats`; `None` for an empty grid.
///
/// The counter is 1-based (see [`PositionTracker`]): beat N is grid entry
/// N - 1, and 0 is the lead-in, from track-ms 0 up to the first beat. A beat
/// lasts until the next grid entry, so tempo changes are followed; past the
/// grid's last entry the beats run on at `beat_ms`.
fn grid_beat_span(beats: &[BeatGridBeat], beat_count: u32, beat_ms: f64) -> Option<BeatSpan> {
    let first = beats.first()?;
    let last = beats.last()?;
    let (start_ms, length_ms) = match beat_count.checked_sub(1) {
        None => (0.0, first.offset_ms),
        Some(index) => {
            let index = index as usize;
            match beats.get(index) {
                Some(beat) => {
                    let length_ms = beats
                        .get(index + 1)
                        .map_or(beat_ms, |next| next.offset_ms - beat.offset_ms);
                    (beat.offset_ms, length_ms)
                }
                None => {
                    let beats_past_last = (index + 1 - beats.len()) as f64;
                    (last.offset_ms + beats_past_last * beat_ms, beat_ms)
                }
            }
        }
    };
    Some(BeatSpan {
        start_ms,
        length_ms: length_ms.max(0.0),
        quality: PositionQuality::BeatGrid,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use prolink::status::{MediaSlot, RawPlayState};

    fn status(beat_count: Option<u32>, track_bpm: Option<f64>) -> DeviceStatus {
        DeviceStatus {
            device_number: 1,
            track_source_device: 1,
            play_state: RawPlayState::Playing,
            is_master: false,
            is_synced: false,
            is_on_air: false,
            media_slot: MediaSlot::Usb,
            track_type: 1,
            pitch_percent: 0.0,
            track_bpm,
            beat_count,
            beat_in_bar: Some(1),
            track_id: Some(1),
            loop_bounds: None,
        }
    }

    /// A deck playing beat `beat_count` of a 120 BPM track: 500ms per beat.
    fn playing_beat(beat_count: u32) -> DeviceStatus {
        status(Some(beat_count), Some(120.0))
    }

    /// The track every status in these tests names as loaded.
    fn loaded() -> TrackRef {
        track(1)
    }

    fn track(track_id: u32) -> TrackRef {
        TrackRef {
            source_device: 1,
            slot: "usb".to_owned(),
            track_id,
        }
    }

    /// A grid with one beat at each of `offsets_ms`.
    fn grid(offsets_ms: &[f64]) -> Vec<BeatGridBeat> {
        offsets_ms
            .iter()
            .zip((1..=4).cycle())
            .map(|(&offset_ms, beat_in_measure)| BeatGridBeat {
                offset_ms,
                beat_in_measure,
            })
            .collect()
    }

    /// A 120 BPM grid (a beat every 500ms) whose first beat is `first_ms` in.
    fn steady_grid(first_ms: f64, beats: usize) -> Vec<BeatGridBeat> {
        let offsets: Vec<f64> = (0..beats)
            .map(|beat| first_ms + beat as f64 * 500.0)
            .collect();
        grid(&offsets)
    }

    fn paused_beat(beat_count: u32) -> DeviceStatus {
        DeviceStatus {
            play_state: RawPlayState::PausedAway,
            ..playing_beat(beat_count)
        }
    }

    fn estimate_of(
        tracker: &mut PositionTracker,
        status: &DeviceStatus,
        now_ms: f64,
    ) -> PositionEstimate {
        tracker
            .estimate(1, status, Some(&loaded()), now_ms)
            .expect("the status carries a beat count and BPM")
    }

    fn estimate_at(tracker: &mut PositionTracker, status: &DeviceStatus, now_ms: f64) -> f64 {
        estimate_of(tracker, status, now_ms).position_ms
    }

    #[test]
    fn no_signal_at_all_is_unknown() {
        let mut tracker = PositionTracker::new();
        assert_eq!(
            tracker.estimate(1, &status(None, None), Some(&loaded()), 0.0),
            None
        );
    }

    #[test]
    fn approximates_from_beat_count_and_bpm_at_constant_tempo() {
        let mut tracker = PositionTracker::new();
        // 64 beats at 120 BPM (2 beats/sec) = 32 seconds in.
        let estimate = estimate_at(&mut tracker, &status(Some(64), Some(120.0)), 0.0);
        assert!((estimate - 32_000.0).abs() < 0.01);
    }

    #[test]
    fn a_fresh_precise_reading_takes_priority_over_the_approximation() {
        let mut tracker = PositionTracker::new();
        tracker.record_precise_position(1, 50_000, 1_000.0);
        let estimate = estimate_at(&mut tracker, &status(Some(64), Some(120.0)), 1_100.0);
        assert_eq!(estimate, 50_000.0);
    }

    #[test]
    fn a_stale_precise_reading_falls_back_to_the_approximation() {
        let mut tracker = PositionTracker::new();
        tracker.record_precise_position(1, 50_000, 1_000.0);
        let estimate = estimate_at(
            &mut tracker,
            &status(Some(64), Some(120.0)),
            1_000.0 + PRECISE_POSITION_FRESHNESS_MS + 1.0,
        );
        assert!((estimate - 32_000.0).abs() < 0.01);
    }

    #[test]
    fn forgetting_a_deck_drops_its_precise_reading() {
        let mut tracker = PositionTracker::new();
        tracker.record_precise_position(1, 50_000, 1_000.0);
        tracker.forget(1);
        let estimate = estimate_at(&mut tracker, &status(Some(64), Some(120.0)), 1_000.0);
        assert!((estimate - 32_000.0).abs() < 0.01);
    }

    #[test]
    fn forgetting_a_deck_drops_its_beat_grid() {
        let mut tracker = PositionTracker::new();
        tracker.set_beat_grid(1, loaded(), steady_grid(1_800.0, 8));
        tracker.forget(1);
        assert_eq!(tracker.beat_grid_track(1), None);
        assert_eq!(
            estimate_of(&mut tracker, &paused_beat(1), 0.0).quality,
            PositionQuality::Estimate
        );
    }

    #[test]
    fn zero_bpm_never_divides_by_zero() {
        let mut tracker = PositionTracker::new();
        assert_eq!(
            tracker.estimate(1, &status(Some(10), Some(0.0)), Some(&loaded()), 0.0),
            None
        );
    }

    #[test]
    fn a_beat_grid_with_lead_in_silence_places_beat_one_after_zero() {
        let mut tracker = PositionTracker::new();
        tracker.set_beat_grid(1, loaded(), steady_grid(1_800.0, 8));

        let beat_one = estimate_of(&mut tracker, &paused_beat(1), 0.0);
        assert_eq!(
            beat_one.position_ms, 1_800.0,
            "beat 1 is the grid's first beat"
        );
        assert_eq!(beat_one.quality, PositionQuality::BeatGrid);
        assert_eq!(estimate_at(&mut tracker, &paused_beat(2), 0.0), 2_300.0);
        assert_eq!(
            estimate_at(&mut tracker, &paused_beat(0), 0.0),
            0.0,
            "the counter reads 0 during the lead-in"
        );

        // Playing through the lead-in runs up to the first beat, no further.
        estimate_at(&mut tracker, &playing_beat(0), 1_000.0);
        assert_eq!(
            estimate_at(&mut tracker, &playing_beat(0), 2_000.0),
            1_000.0
        );
        assert_eq!(
            estimate_at(&mut tracker, &playing_beat(0), 4_000.0),
            1_800.0
        );
    }

    #[test]
    fn a_grid_with_a_tempo_change_follows_the_grid_not_the_bpm() {
        let mut tracker = PositionTracker::new();
        // 120 BPM for three beats, then 150 BPM; the status still says 120.
        tracker.set_beat_grid(1, loaded(), grid(&[0.0, 500.0, 1_000.0, 1_400.0, 1_800.0]));

        assert_eq!(estimate_at(&mut tracker, &paused_beat(5), 0.0), 1_800.0);

        // Beat 4 lasts 400ms on the grid, not the 500ms the BPM implies.
        estimate_at(&mut tracker, &playing_beat(4), 1_000.0);
        assert_eq!(
            estimate_at(&mut tracker, &playing_beat(4), 1_300.0),
            1_700.0
        );
        assert_eq!(
            estimate_at(&mut tracker, &playing_beat(4), 1_600.0),
            1_800.0
        );

        // Past the grid's last entry the beats run on at the BPM.
        assert_eq!(estimate_at(&mut tracker, &paused_beat(7), 0.0), 2_800.0);
    }

    #[test]
    fn a_grid_for_another_track_is_never_used() {
        let mut tracker = PositionTracker::new();
        // The previous track's grid, still shown while the new one loads.
        tracker.set_beat_grid(1, track(1), steady_grid(1_800.0, 8));

        let new_track = track(2);
        let estimate = tracker
            .estimate(1, &paused_beat(1), Some(&new_track), 0.0)
            .expect("the status carries a beat count and BPM");
        assert_eq!(estimate.quality, PositionQuality::Estimate);
        assert_eq!(estimate.position_ms, 500.0);

        let unknown = tracker
            .estimate(1, &paused_beat(1), None, 0.0)
            .expect("the status carries a beat count and BPM");
        assert_eq!(unknown.quality, PositionQuality::Estimate);
    }

    #[test]
    fn a_precise_reading_older_than_150ms_falls_back_to_the_grid() {
        let mut tracker = PositionTracker::new();
        tracker.set_beat_grid(1, loaded(), steady_grid(1_800.0, 8));
        tracker.record_precise_position(1, 50_000, 1_000.0);

        let at_the_limit = estimate_of(&mut tracker, &paused_beat(1), 1_150.0);
        assert_eq!(at_the_limit.quality, PositionQuality::Precise);
        assert_eq!(at_the_limit.position_ms, 50_000.0);

        let stale = estimate_of(&mut tracker, &paused_beat(1), 1_151.0);
        assert_eq!(stale.quality, PositionQuality::BeatGrid);
        assert_eq!(stale.position_ms, 1_800.0);
    }

    #[test]
    fn the_estimate_tag_is_precise_grid_or_estimate() {
        let mut tracker = PositionTracker::new();
        assert_eq!(
            estimate_of(&mut tracker, &paused_beat(1), 0.0).quality,
            PositionQuality::Estimate
        );

        tracker.set_beat_grid(1, loaded(), steady_grid(0.0, 8));
        assert_eq!(
            estimate_of(&mut tracker, &paused_beat(1), 0.0).quality,
            PositionQuality::BeatGrid
        );

        tracker.record_precise_position(1, 50_000, 0.0);
        assert_eq!(
            estimate_of(&mut tracker, &paused_beat(1), 0.0).quality,
            PositionQuality::Precise
        );
    }

    #[test]
    fn an_empty_grid_is_no_grid() {
        let mut tracker = PositionTracker::new();
        tracker.set_beat_grid(1, loaded(), Vec::new());
        assert_eq!(
            estimate_of(&mut tracker, &paused_beat(1), 0.0).quality,
            PositionQuality::Estimate
        );
    }

    #[test]
    fn a_playing_deck_moves_on_between_statuses_reporting_the_same_beat() {
        // Re-reading the bare beat start from each status pulled the
        // position back to 32 000 every 200ms.
        let mut tracker = PositionTracker::new();
        let beat = playing_beat(64);
        assert_eq!(estimate_at(&mut tracker, &beat, 1_000.0), 32_000.0);
        assert_eq!(estimate_at(&mut tracker, &beat, 1_200.0), 32_200.0);
        assert_eq!(estimate_at(&mut tracker, &beat, 1_400.0), 32_400.0);
    }

    #[test]
    fn the_estimate_never_runs_past_the_next_beat() {
        let mut tracker = PositionTracker::new();
        estimate_at(&mut tracker, &playing_beat(64), 1_000.0);
        // 700ms into a 500ms beat: the next beat just hasn't been reported.
        assert_eq!(
            estimate_at(&mut tracker, &playing_beat(64), 1_700.0),
            32_500.0
        );
        // And once it is, the position carries on from there, not back.
        assert_eq!(
            estimate_at(&mut tracker, &playing_beat(65), 1_800.0),
            32_500.0
        );
    }

    #[test]
    fn a_beat_packet_pins_the_next_beat_to_when_it_began() {
        let mut tracker = PositionTracker::new();
        estimate_at(&mut tracker, &playing_beat(64), 1_000.0);
        tracker.record_beat(1, 1_480.0);
        // The status only reports beat 65 120ms after its packet marked it.
        assert_eq!(
            estimate_at(&mut tracker, &playing_beat(65), 1_600.0),
            32_620.0
        );
    }

    #[test]
    fn a_beat_packet_older_than_a_beat_is_not_trusted() {
        let mut tracker = PositionTracker::new();
        estimate_at(&mut tracker, &playing_beat(64), 1_000.0);
        tracker.record_beat(1, 1_050.0);
        assert_eq!(
            estimate_at(&mut tracker, &playing_beat(65), 1_600.0),
            32_500.0
        );
    }

    #[test]
    fn a_jump_is_pinned_to_the_status_that_reports_it() {
        let mut tracker = PositionTracker::new();
        estimate_at(&mut tracker, &playing_beat(64), 1_000.0);
        tracker.record_beat(1, 1_480.0);
        // A hot cue back to beat 10: that beat packet belonged to beat 65.
        assert_eq!(
            estimate_at(&mut tracker, &playing_beat(10), 1_600.0),
            5_000.0
        );
    }

    #[test]
    fn the_estimate_moves_at_the_decks_pitched_speed() {
        let mut tracker = PositionTracker::new();
        let pitched = DeviceStatus {
            pitch_percent: 10.0,
            ..playing_beat(64)
        };
        estimate_at(&mut tracker, &pitched, 1_000.0);
        assert!((estimate_at(&mut tracker, &pitched, 1_200.0) - 32_220.0).abs() < 0.01);
    }

    #[test]
    fn a_paused_deck_holds_its_beat_and_starts_afresh_when_played() {
        let mut tracker = PositionTracker::new();
        estimate_at(&mut tracker, &playing_beat(64), 1_000.0);
        tracker.record_beat(1, 1_100.0);
        let paused = DeviceStatus {
            play_state: RawPlayState::PausedAway,
            ..playing_beat(64)
        };
        assert_eq!(estimate_at(&mut tracker, &paused, 1_200.0), 32_000.0);
        assert_eq!(estimate_at(&mut tracker, &paused, 9_000.0), 32_000.0);
        // Eight seconds of pause count for nothing, and neither does the
        // beat packet from before it.
        assert_eq!(
            estimate_at(&mut tracker, &playing_beat(64), 10_000.0),
            32_000.0
        );
        assert_eq!(
            estimate_at(&mut tracker, &playing_beat(64), 10_200.0),
            32_200.0
        );
    }

    #[test]
    fn falling_back_from_a_precise_reading_starts_a_fresh_anchor() {
        let mut tracker = PositionTracker::new();
        estimate_at(&mut tracker, &playing_beat(64), 1_000.0);
        tracker.record_precise_position(1, 50_000, 2_000.0);
        estimate_at(&mut tracker, &playing_beat(64), 2_000.0);
        // Back on the same beat count well after the old anchor would have
        // capped at the next beat.
        let fallback_at = 2_000.0 + PRECISE_POSITION_FRESHNESS_MS + 1.0;
        assert_eq!(
            estimate_at(&mut tracker, &playing_beat(64), fallback_at),
            32_000.0
        );
    }
}
