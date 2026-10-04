//! Estimates each deck's playhead position in milliseconds.

use std::collections::HashMap;

use decktector_core::model::DeckId;
use prolink::status::DeviceStatus;

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
#[derive(Debug, Default)]
pub struct PositionTracker {
    precise: HashMap<DeckId, PreciseReading>,
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

    /// Drops a deck's tracked state. Call this when its device disconnects,
    /// so a stale precise-position reading never survives to be reused if
    /// the same number is later assigned to a different player.
    pub fn forget(&mut self, deck_id: DeckId) {
        self.precise.remove(&deck_id);
    }

    /// Best current playhead estimate for a status update, in ms. `None`
    /// when neither a fresh precise-position reading nor enough of the
    /// status packet (beat count, BPM) is available to approximate from.
    pub fn estimate(&self, deck_id: DeckId, status: &DeviceStatus, now_ms: f64) -> Option<f64> {
        if let Some(reading) = self.precise.get(&deck_id) {
            if now_ms - reading.received_at_ms <= PRECISE_POSITION_FRESHNESS_MS {
                return Some(reading.position_ms);
            }
        }
        let beat_count = status.beat_count?;
        let bpm = status.track_bpm?;
        (bpm > 0.0).then(|| f64::from(beat_count) * 60_000.0 / bpm)
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
        }
    }

    #[test]
    fn no_signal_at_all_is_unknown() {
        let tracker = PositionTracker::new();
        assert_eq!(tracker.estimate(1, &status(None, None), 0.0), None);
    }

    #[test]
    fn approximates_from_beat_count_and_bpm_at_constant_tempo() {
        let tracker = PositionTracker::new();
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
        let tracker = PositionTracker::new();
        assert_eq!(tracker.estimate(1, &status(Some(10), Some(0.0)), 0.0), None);
    }
}
