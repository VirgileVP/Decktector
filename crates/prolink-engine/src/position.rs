//! Estimates each deck's playhead position in milliseconds.

use std::collections::HashMap;

use decktector_core::model::{DeckId, PlayState};
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

/// Estimates each deck's playhead in milliseconds from whatever position
/// signal is currently available.
///
/// A CDJ-3000's precise-position packets are used directly while fresh —
/// the only genuinely absolute position this protocol offers. Every other
/// reading (older hardware, or a CDJ-3000 between precise-position packets)
/// falls back to `beat_count * (60_000 / bpm)`, which assumes beat 1 lands
/// at track-ms 0 and tempo stays constant. That's a real approximation, not
/// the beat-grid-anchored estimate `DeckState::playhead_ms`'s own doc
/// comment describes as the long-term design — there is no beat grid yet
/// (it arrives with track metadata, a later phase). Revisit this once that
/// lands: a beat grid corrects for lead-in silence and tempo changes a
/// constant-BPM guess can't.
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
    }

    /// Best current playhead estimate for a status update, in ms. `None`
    /// when neither a fresh precise-position reading nor enough of the
    /// status packet (beat count, BPM) is available to approximate from.
    pub fn estimate(&mut self, deck_id: DeckId, status: &DeviceStatus, now_ms: f64) -> Option<f64> {
        if let Some(reading) = self.precise.get(&deck_id) {
            if now_ms - reading.received_at_ms <= PRECISE_POSITION_FRESHNESS_MS {
                // The beat count may have moved anywhere meanwhile: the next
                // fallback must not resume from an anchor this old.
                self.beat_anchors.remove(&deck_id);
                return Some(reading.position_ms);
            }
        }
        let beat_count = status.beat_count?;
        let bpm = status.track_bpm.filter(|bpm| *bpm > 0.0)?;
        let beat_ms = 60_000.0 / bpm;
        let beat_start_ms = f64::from(beat_count) * beat_ms;
        let speed = 1.0 + status.pitch_percent / 100.0;

        if play_state(status.play_state) != PlayState::Playing || speed <= 0.0 {
            // A stopped deck sits wherever it stopped. Neither the time spent
            // there nor a beat packet from before the stop may count once it
            // plays again.
            self.beat_anchors.remove(&deck_id);
            self.last_beat_at_ms.remove(&deck_id);
            return Some(beat_start_ms);
        }

        let anchor = self.anchor_beat(deck_id, beat_count, beat_ms / speed, now_ms);
        let into_beat_ms = ((now_ms - anchor.began_at_ms) * speed).clamp(0.0, beat_ms);
        Some(beat_start_ms + into_beat_ms)
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

    fn estimate_at(tracker: &mut PositionTracker, status: &DeviceStatus, now_ms: f64) -> f64 {
        tracker
            .estimate(1, status, now_ms)
            .expect("the status carries a beat count and BPM")
    }

    #[test]
    fn no_signal_at_all_is_unknown() {
        let mut tracker = PositionTracker::new();
        assert_eq!(tracker.estimate(1, &status(None, None), 0.0), None);
    }

    #[test]
    fn approximates_from_beat_count_and_bpm_at_constant_tempo() {
        let mut tracker = PositionTracker::new();
        // 64 beats at 120 BPM (2 beats/sec) = 32 seconds in.
        let estimate = tracker
            .estimate(1, &status(Some(64), Some(120.0)), 0.0)
            .expect("should estimate");
        assert!((estimate - 32_000.0).abs() < 0.01);
    }

    #[test]
    fn a_fresh_precise_reading_takes_priority_over_the_approximation() {
        let mut tracker = PositionTracker::new();
        tracker.record_precise_position(1, 50_000, 1_000.0);
        let estimate = tracker.estimate(1, &status(Some(64), Some(120.0)), 1_100.0);
        assert_eq!(estimate, Some(50_000.0));
    }

    #[test]
    fn a_stale_precise_reading_falls_back_to_the_approximation() {
        let mut tracker = PositionTracker::new();
        tracker.record_precise_position(1, 50_000, 1_000.0);
        let estimate = tracker
            .estimate(
                1,
                &status(Some(64), Some(120.0)),
                1_000.0 + PRECISE_POSITION_FRESHNESS_MS + 1.0,
            )
            .expect("should fall back");
        assert!((estimate - 32_000.0).abs() < 0.01);
    }

    #[test]
    fn forgetting_a_deck_drops_its_precise_reading() {
        let mut tracker = PositionTracker::new();
        tracker.record_precise_position(1, 50_000, 1_000.0);
        tracker.forget(1);
        let estimate = tracker.estimate(1, &status(Some(64), Some(120.0)), 1_000.0);
        assert!((estimate.expect("should still approximate") - 32_000.0).abs() < 0.01);
    }

    #[test]
    fn zero_bpm_never_divides_by_zero() {
        let mut tracker = PositionTracker::new();
        assert_eq!(tracker.estimate(1, &status(Some(10), Some(0.0)), 0.0), None);
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
