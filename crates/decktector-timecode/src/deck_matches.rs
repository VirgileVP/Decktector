//! Where every loaded deck stands against the show, kept up to date tick by
//! tick — not only the timecode source: the deck bars show each deck's match.
//!
//! A deck's resolution is worked out when its track loads and then left
//! alone ("fixed at load"), except while it is provisional (pending, or timed
//! out waiting for metadata) or when what it was resolved against changed:
//! a new show table, or another output frame rate. Reading a deck's metadata
//! clones its waveform, so it happens only then, never once per tick.

use std::collections::HashMap;
use std::time::Instant;

use decktector_core::model::{DeckId, DeckState, PositionQuality, TrackRef};
use decktector_core::EngineHandle;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::show::{resolve_deck, DeckResolution, TimecodeShow, UnidentifiedReason};
use crate::FrameRate;

/// What the deck bars show for one deck.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "DeckMatchStatus.ts")]
pub struct DeckMatchStatus {
    pub deck_id: DeckId,
    pub resolution: DeckResolution,
    /// The title of the row the deck is on, or would be once timed.
    pub row_title: Option<String>,
    /// Where the deck's timeline starts, in frames at the show's rate: its
    /// row's start when matched, the show's fallback start when falling back.
    #[ts(type = "number | null")]
    pub start_frames: Option<u64>,
    pub position_quality: Option<PositionQuality>,
}

/// What a resolution was worked out against; when it changes, it is redone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ResolvedAgainst {
    show_revision: Option<u64>,
    output_rate: FrameRate,
}

/// One deck's load and where it stands.
#[derive(Clone, Debug)]
struct DeckEntry {
    loaded: Option<TrackRef>,
    /// When this load was first seen: the start of its wait for metadata.
    loaded_at: Instant,
    resolution: DeckResolution,
    resolved_against: ResolvedAgainst,
}

/// Still waiting for metadata: worked out again on every refresh until the
/// deck's metadata names the loaded track.
fn is_provisional(resolution: &DeckResolution) -> bool {
    match resolution {
        DeckResolution::Pending => true,
        DeckResolution::Fallback { reason } | DeckResolution::Cut { reason } => {
            *reason == UnidentifiedReason::PendingTimeout
        }
        _ => false,
    }
}

/// Every deck's resolution, and the statuses last published from them.
#[derive(Debug, Default)]
pub(crate) struct DeckMatches {
    decks: HashMap<DeckId, DeckEntry>,
    published: Vec<DeckMatchStatus>,
}

impl DeckMatches {
    /// Where `deck_id` stands, if it has reported.
    pub(crate) fn resolution(&self, deck_id: DeckId) -> Option<&DeckResolution> {
        self.decks.get(&deck_id).map(|entry| &entry.resolution)
    }

    /// Brings every deck in `decks` up to date, reading its metadata through
    /// `handle` only when its resolution has to be worked out. Returns the
    /// statuses when they differ from the last ones returned.
    pub(crate) fn refresh(
        &mut self,
        handle: &EngineHandle,
        decks: &[DeckState],
        show: Option<&TimecodeShow>,
        output_rate: FrameRate,
        now: Instant,
    ) -> Option<Vec<DeckMatchStatus>> {
        let against = ResolvedAgainst {
            show_revision: show.map(|show| show.revision),
            output_rate,
        };
        self.decks
            .retain(|deck_id, _| decks.iter().any(|deck| deck.deck_id == *deck_id));
        for deck in decks {
            self.refresh_deck(handle, deck, show, against, now);
        }

        let statuses: Vec<DeckMatchStatus> = decks
            .iter()
            .filter_map(|deck| {
                let entry = self.decks.get(&deck.deck_id)?;
                Some(status_of(deck, &entry.resolution, show))
            })
            .collect();
        if statuses == self.published {
            return None;
        }
        self.published.clone_from(&statuses);
        Some(statuses)
    }

    fn refresh_deck(
        &mut self,
        handle: &EngineHandle,
        deck: &DeckState,
        show: Option<&TimecodeShow>,
        against: ResolvedAgainst,
        now: Instant,
    ) {
        let previous = self
            .decks
            .remove(&deck.deck_id)
            .filter(|entry| entry.loaded == deck.loaded_track);
        let is_up_to_date = previous.as_ref().is_some_and(|entry| {
            entry.resolved_against == against && !is_provisional(&entry.resolution)
        });
        let entry = match previous {
            Some(entry) if is_up_to_date => entry,
            previous => {
                let loaded_at = previous.as_ref().map_or(now, |entry| entry.loaded_at);
                let loaded = deck.loaded_track.clone();
                // Only metadata that already describes the loaded track is worth
                // cloning: anything else is the previous track's.
                let info = loaded
                    .as_ref()
                    .filter(|loaded| {
                        handle.deck_track_source(deck.deck_id).as_ref() == Some(loaded)
                    })
                    .and_then(|_| handle.deck_track(deck.deck_id));
                let pending_for_ms =
                    u64::try_from(now.saturating_duration_since(loaded_at).as_millis())
                        .unwrap_or(u64::MAX);
                let resolution = resolve_deck(
                    show,
                    against.output_rate,
                    loaded.as_ref(),
                    info.as_ref(),
                    previous.as_ref().map(|entry| &entry.resolution),
                    pending_for_ms,
                );
                DeckEntry {
                    loaded,
                    loaded_at,
                    resolution,
                    resolved_against: against,
                }
            }
        };
        self.decks.insert(deck.deck_id, entry);
    }
}

/// The deck-bar view of `deck` standing at `resolution`.
fn status_of(
    deck: &DeckState,
    resolution: &DeckResolution,
    show: Option<&TimecodeShow>,
) -> DeckMatchStatus {
    let row = |row_id: &str| show.and_then(|show| show.rows.iter().find(|row| row.id == row_id));
    let row_id = match resolution {
        DeckResolution::Matched { row_id }
        | DeckResolution::Fallback {
            reason: UnidentifiedReason::BlankStart { row_id },
        }
        | DeckResolution::Cut {
            reason: UnidentifiedReason::BlankStart { row_id },
        } => Some(row_id.as_str()),
        _ => None,
    };
    let start_frames = match resolution {
        DeckResolution::Matched { row_id } => row(row_id).and_then(|row| row.start_frames),
        DeckResolution::Fallback { .. } => show.map(|show| show.fallback_start_frames),
        _ => None,
    };
    DeckMatchStatus {
        deck_id: deck.deck_id,
        resolution: resolution.clone(),
        row_title: row_id.and_then(row).map(|row| row.title.clone()),
        start_frames,
        position_quality: deck.position_quality,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::show::ShowRowEntry;
    use decktector_core::model::{PlayState, TrackInfo};

    const RATE: FrameRate = FrameRate::Ebu25;
    const FRAMES_PER_HOUR: u64 = 3600 * 25;

    fn track_ref(track_id: u32) -> TrackRef {
        TrackRef {
            source_device: 1,
            slot: "usb".to_owned(),
            track_id,
        }
    }

    fn deck(deck_id: DeckId, track_id: Option<u32>) -> DeckState {
        DeckState {
            deck_id,
            play_state: PlayState::Paused,
            is_master: false,
            is_synced: false,
            is_on_air: false,
            pitch: 0.0,
            track_bpm: None,
            effective_bpm: None,
            beat_in_measure: None,
            track_id,
            loaded_track: track_id.map(track_ref),
            playhead_ms: Some(0.0),
            position_quality: Some(PositionQuality::BeatGrid),
            reported_at: 0.0,
            active_loop: None,
        }
    }

    fn metadata(track_id: u32, title: &str) -> TrackInfo {
        TrackInfo {
            track_id,
            source: Some(track_ref(track_id)),
            title: title.to_owned(),
            artist: Some("Artist".to_owned()),
            album: None,
            genre: None,
            key: None,
            label: None,
            comment: None,
            duration_ms: 300_000.0,
            bpm: None,
            artwork: None,
            beat_grid: None,
            waveform: None,
            cues: Vec::new(),
            events: Vec::new(),
        }
    }

    fn row(id: &str, title: &str, start_frames: Option<u64>) -> ShowRowEntry {
        ShowRowEntry {
            id: id.to_owned(),
            title: title.to_owned(),
            artist: "Artist".to_owned(),
            duration_seconds: 300,
            start_frames,
            trim_ms: 0,
        }
    }

    fn table() -> TimecodeShow {
        TimecodeShow {
            revision: 1,
            frame_rate: RATE,
            fallback_start_frames: 23 * FRAMES_PER_HOUR,
            cut_when_unidentified: false,
            rows: vec![
                row("strobe", "Strobe", Some(FRAMES_PER_HOUR)),
                row("blank", "Blank", None),
            ],
        }
    }

    #[test]
    fn statuses_are_published_only_when_they_change() {
        let handle = EngineHandle::new();
        handle.update_deck_track(1, Some(metadata(15, "Strobe")));
        let decks = [deck(1, Some(15))];
        let show = table();
        let mut matches = DeckMatches::default();
        let now = Instant::now();

        let first = matches.refresh(&handle, &decks, Some(&show), RATE, now);
        assert_eq!(
            first,
            Some(vec![DeckMatchStatus {
                deck_id: 1,
                resolution: DeckResolution::Matched {
                    row_id: "strobe".to_owned()
                },
                row_title: Some("Strobe".to_owned()),
                start_frames: Some(FRAMES_PER_HOUR),
                position_quality: Some(PositionQuality::BeatGrid),
            }])
        );
        assert_eq!(
            matches.refresh(&handle, &decks, Some(&show), RATE, now),
            None
        );

        let precise = [DeckState {
            position_quality: Some(PositionQuality::Precise),
            ..deck(1, Some(15))
        }];
        assert!(matches
            .refresh(&handle, &precise, Some(&show), RATE, now)
            .is_some());
    }

    #[test]
    fn a_deck_on_a_blank_row_names_the_row_and_runs_from_the_fallback() {
        let handle = EngineHandle::new();
        handle.update_deck_track(2, Some(metadata(16, "Blank")));
        let mut matches = DeckMatches::default();

        let statuses = matches
            .refresh(
                &handle,
                &[deck(2, Some(16))],
                Some(&table()),
                RATE,
                Instant::now(),
            )
            .expect("a first status");
        assert_eq!(statuses[0].row_title.as_deref(), Some("Blank"));
        assert_eq!(statuses[0].start_frames, Some(23 * FRAMES_PER_HOUR));
    }

    #[test]
    fn a_deck_that_left_is_forgotten() {
        let handle = EngineHandle::new();
        let mut matches = DeckMatches::default();
        let now = Instant::now();
        matches.refresh(
            &handle,
            &[deck(1, None), deck(2, None)],
            Some(&table()),
            RATE,
            now,
        );
        assert_eq!(matches.resolution(2), Some(&DeckResolution::None));

        let statuses = matches.refresh(&handle, &[deck(1, None)], Some(&table()), RATE, now);
        assert_eq!(statuses.map(|statuses| statuses.len()), Some(1));
        assert_eq!(matches.resolution(2), None);
    }

    #[test]
    fn a_deck_waits_for_its_metadata_from_when_its_track_loaded() {
        let handle = EngineHandle::new();
        let mut matches = DeckMatches::default();
        let loaded_at = Instant::now();
        let decks = [deck(1, Some(15))];

        matches.refresh(&handle, &decks, Some(&table()), RATE, loaded_at);
        assert_eq!(matches.resolution(1), Some(&DeckResolution::Pending));

        let later = loaded_at + std::time::Duration::from_millis(crate::PENDING_TIMEOUT_MS);
        matches.refresh(&handle, &decks, Some(&table()), RATE, later);
        assert_eq!(
            matches.resolution(1),
            Some(&DeckResolution::Fallback {
                reason: UnidentifiedReason::PendingTimeout
            })
        );
    }
}
