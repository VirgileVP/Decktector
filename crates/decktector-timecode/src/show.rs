//! The show table the renderer pushes, and what it means for one deck.
//!
//! The renderer owns the show — editing, files, persistence — and pushes the
//! whole table, start timecodes already turned into frames at the show's own
//! rate. Here each loaded deck is matched against it ([`resolve_deck`]) and a
//! matched deck's timeline read off its row ([`timeline_for`]). Everything is
//! pure: the service decides when to call it and what to cache.

use decktector_core::model::{is_identified, TrackInfo, TrackRef};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::identity::Identity;
use crate::{FrameRate, Timeline};

/// How long a freshly loaded track may stay unmatched by metadata before it
/// is treated as unidentified. The NFS metadata route alone may take 20 s, so
/// metadata landing after this is expected, and still resolves normally.
pub const PENDING_TIMEOUT_MS: u64 = 10_000;

const MS_PER_SECOND: f64 = 1_000.0;

/// Tonight's show as the timecode service uses it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "TimecodeShow.ts")]
pub struct TimecodeShow {
    /// Increases with every push, so a reader can tell two tables apart cheaply.
    #[ts(type = "number")]
    pub revision: u64,
    /// The show's own rate: its frame counts mean nothing at another one.
    pub frame_rate: FrameRate,
    /// Frames, computed from the renderer's label at push time with the show's fps.
    #[ts(type = "number")]
    pub fallback_start_frames: u64,
    pub cut_when_unidentified: bool,
    pub rows: Vec<ShowRowEntry>,
}

/// One track of the show.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "ShowRowEntry.ts")]
pub struct ShowRowEntry {
    pub id: String,
    pub title: String,
    /// The track's own artist; empty when unknown.
    pub artist: String,
    /// Whole seconds; 0 when unknown, and an unknown track never matches.
    pub duration_seconds: u32,
    /// Frames from `00:00:00:00` at the show's rate; null while the row is not timed yet.
    #[ts(type = "number | null")]
    pub start_frames: Option<u64>,
    /// Signed nudge: positive runs the timecode ahead, so cues fire earlier.
    #[ts(type = "number")]
    pub trim_ms: i64,
}

impl TimecodeShow {
    fn row(&self, id: &str) -> Option<&ShowRowEntry> {
        self.rows.iter().find(|row| row.id == id)
    }
}

/// Why a deck is not on a row's timeline.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(
    export,
    export_to = "UnidentifiedReason.ts",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum UnidentifiedReason {
    /// No row is this track.
    NoMatch,
    /// The track's metadata gives no duration, so it can match nothing.
    NoDuration,
    /// Several rows are this track, with different starts.
    Ambiguous { row_ids: Vec<String> },
    /// The row that is this track has no start timecode yet. The row is kept
    /// so that typing its start puts the deck on it without a re-match.
    BlankStart { row_id: String },
    /// The deck's metadata never arrived within [`PENDING_TIMEOUT_MS`].
    PendingTimeout,
}

/// Where one deck stands against the show.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(
    export,
    export_to = "DeckResolution.ts",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum DeckResolution {
    /// The deck holds no track.
    None,
    /// No show is loaded: the deck runs from `00:00:00:00`, as before shows.
    Inactive,
    /// The deck's metadata still describes another track (or none yet).
    Pending,
    /// The show's fps differs from the output rate: the output is cut.
    RateMismatch,
    Matched {
        row_id: String,
    },
    /// Unidentified, running from the show's fallback start.
    Fallback {
        reason: UnidentifiedReason,
    },
    /// Unidentified, with the output stopped as the show asks.
    Cut {
        reason: UnidentifiedReason,
    },
}

/// What a deck's load was found to be, before the show's switches apply.
enum Outcome {
    Matched(String),
    Unidentified(UnidentifiedReason),
}

/// The outcome `cached` already settled for this load, if it still holds in
/// `show`: a match stands while its row exists — whatever its start and trim
/// became — and an unidentified load stays so ("fixed at load"). Pending
/// and a pending timeout are provisional and never reused; a row that left
/// the table (a Replace) means matching once more.
fn settled_outcome(cached: &DeckResolution, show: &TimecodeShow) -> Option<Outcome> {
    let reason = match cached {
        DeckResolution::Matched { row_id } => {
            return show.row(row_id).map(|row| Outcome::Matched(row.id.clone()))
        }
        DeckResolution::Fallback { reason } | DeckResolution::Cut { reason } => reason,
        DeckResolution::None
        | DeckResolution::Inactive
        | DeckResolution::Pending
        | DeckResolution::RateMismatch => return None,
    };
    match reason {
        UnidentifiedReason::BlankStart { row_id } => {
            show.row(row_id).map(|row| Outcome::Matched(row.id.clone()))
        }
        UnidentifiedReason::Ambiguous { row_ids } => row_ids
            .iter()
            .all(|row_id| show.row(row_id).is_some())
            .then(|| Outcome::Unidentified(reason.clone())),
        UnidentifiedReason::NoMatch | UnidentifiedReason::NoDuration => {
            Some(Outcome::Unidentified(reason.clone()))
        }
        UnidentifiedReason::PendingTimeout => None,
    }
}

/// `info`'s duration in whole seconds, as show rows count it.
fn duration_seconds(info: &TrackInfo) -> u32 {
    (info.duration_ms / MS_PER_SECOND).round() as u32
}

/// Matches identified metadata against every row.
fn match_rows(info: &TrackInfo, show: &TimecodeShow) -> Outcome {
    let track = Identity::new(
        &info.title,
        info.artist.as_deref().unwrap_or_default(),
        duration_seconds(info),
    );
    if track.duration_seconds == 0 {
        return Outcome::Unidentified(UnidentifiedReason::NoDuration);
    }
    let mut matching: Vec<&ShowRowEntry> = show
        .rows
        .iter()
        .filter(|row| Identity::new(&row.title, &row.artist, row.duration_seconds).matches(&track))
        .collect();
    match matching.len() {
        0 => Outcome::Unidentified(UnidentifiedReason::NoMatch),
        1 => Outcome::Matched(matching.remove(0).id.clone()),
        _ => Outcome::Unidentified(UnidentifiedReason::Ambiguous {
            row_ids: matching.into_iter().map(|row| row.id.clone()).collect(),
        }),
    }
}

/// The resolution `outcome` gives under `show`'s current rows and switches:
/// a matched row with no start yet is unidentified (an imported but
/// unprogrammed track must not run at 0), and unidentified is cut or
/// fallback as the show says.
fn resolution_of(outcome: Outcome, show: &TimecodeShow) -> DeckResolution {
    let reason = match outcome {
        Outcome::Matched(row_id) => match show.row(&row_id) {
            Some(row) if row.start_frames.is_some() => return DeckResolution::Matched { row_id },
            _ => UnidentifiedReason::BlankStart { row_id },
        },
        Outcome::Unidentified(reason) => reason,
    };
    if show.cut_when_unidentified {
        DeckResolution::Cut { reason }
    } else {
        DeckResolution::Fallback { reason }
    }
}

/// Where a deck stands against the show — spec steps 1 to 8.
///
/// `loaded` is the track the deck reports loaded and `info` the metadata it
/// shows, which may still be the previous track's. `cached` is this same
/// load's previous resolution, if any (the caller drops it when another
/// track loads), and `pending_for_ms` how long the load has gone without
/// matching metadata.
pub fn resolve_deck(
    show: Option<&TimecodeShow>,
    output_rate: FrameRate,
    loaded: Option<&TrackRef>,
    info: Option<&TrackInfo>,
    cached: Option<&DeckResolution>,
    pending_for_ms: u64,
) -> DeckResolution {
    let Some(show) = show else {
        return DeckResolution::Inactive;
    };
    if show.frame_rate != output_rate {
        return DeckResolution::RateMismatch;
    }
    let Some(loaded) = loaded else {
        return DeckResolution::None;
    };
    let info = match info {
        Some(info) if is_identified(loaded, Some(info)) => info,
        _ if pending_for_ms < PENDING_TIMEOUT_MS => return DeckResolution::Pending,
        _ => {
            return resolution_of(
                Outcome::Unidentified(UnidentifiedReason::PendingTimeout),
                show,
            )
        }
    };
    let outcome = cached
        .and_then(|cached| settled_outcome(cached, show))
        .unwrap_or_else(|| match_rows(info, show));
    resolution_of(outcome, show)
}

/// The timeline a deck resolved as `resolution` runs on: its row's start and
/// trim when matched, the show's fallback start with no trim when falling
/// back, and `00:00:00:00` otherwise — callers hold a pending or cut deck
/// before they ever read it.
pub fn timeline_for(show: &TimecodeShow, resolution: &DeckResolution) -> Timeline {
    match resolution {
        DeckResolution::Matched { row_id } => show
            .row(row_id)
            .and_then(|row| {
                Some(Timeline {
                    start_frames: row.start_frames?,
                    trim_ms: row.trim_ms,
                })
            })
            .unwrap_or_default(),
        DeckResolution::Fallback { .. } => Timeline {
            start_frames: show.fallback_start_frames,
            trim_ms: 0,
        },
        DeckResolution::None
        | DeckResolution::Inactive
        | DeckResolution::Pending
        | DeckResolution::RateMismatch
        | DeckResolution::Cut { .. } => Timeline::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: FrameRate = FrameRate::Ebu25;
    const FRAMES_PER_HOUR: u64 = 3600 * 25;

    fn track_ref(source_device: u8, track_id: u32) -> TrackRef {
        TrackRef {
            source_device,
            slot: "usb".to_owned(),
            track_id,
        }
    }

    /// Deck 1's own stick, track 15: the load every test starts from.
    fn loaded() -> TrackRef {
        track_ref(1, 15)
    }

    fn info(source: TrackRef, title: &str, duration_ms: f64) -> TrackInfo {
        TrackInfo {
            track_id: source.track_id,
            source: Some(source),
            title: title.to_owned(),
            artist: Some("Artist".to_owned()),
            album: None,
            genre: None,
            key: None,
            label: None,
            comment: None,
            duration_ms,
            bpm: None,
            artwork: None,
            beat_grid: None,
            waveform: None,
            cues: Vec::new(),
            events: Vec::new(),
        }
    }

    /// The loaded track's metadata: "Strobe", 300 s.
    fn identified() -> TrackInfo {
        info(loaded(), "Strobe", 300_000.0)
    }

    fn row(id: &str, title: &str, start_hours: Option<u64>) -> ShowRowEntry {
        ShowRowEntry {
            id: id.to_owned(),
            title: title.to_owned(),
            artist: "Artist".to_owned(),
            duration_seconds: 300,
            start_frames: start_hours.map(|hours| hours * FRAMES_PER_HOUR),
            trim_ms: 0,
        }
    }

    fn show(rows: Vec<ShowRowEntry>) -> TimecodeShow {
        TimecodeShow {
            revision: 1,
            frame_rate: RATE,
            fallback_start_frames: 23 * FRAMES_PER_HOUR,
            cut_when_unidentified: false,
            rows,
        }
    }

    fn resolve(
        show: &TimecodeShow,
        info: Option<&TrackInfo>,
        cached: Option<&DeckResolution>,
        pending_for_ms: u64,
    ) -> DeckResolution {
        resolve_deck(
            Some(show),
            RATE,
            Some(&loaded()),
            info,
            cached,
            pending_for_ms,
        )
    }

    fn fallback(reason: UnidentifiedReason) -> DeckResolution {
        DeckResolution::Fallback { reason }
    }

    fn matched(row_id: &str) -> DeckResolution {
        DeckResolution::Matched {
            row_id: row_id.to_owned(),
        }
    }

    #[test]
    fn no_show_means_the_deck_is_inactive_and_the_base_is_zero() {
        let resolution = resolve_deck(None, RATE, Some(&loaded()), Some(&identified()), None, 0);
        assert_eq!(resolution, DeckResolution::Inactive);
        assert_eq!(
            timeline_for(&show(Vec::new()), &resolution),
            Timeline::default()
        );
    }

    #[test]
    fn a_show_in_another_frame_rate_cuts_the_output_and_never_converts() {
        let table = show(vec![row("strobe", "Strobe", Some(1))]);
        for loaded in [Some(loaded()), None] {
            assert_eq!(
                resolve_deck(
                    Some(&table),
                    FrameRate::Smpte30,
                    loaded.as_ref(),
                    Some(&identified()),
                    None,
                    0
                ),
                DeckResolution::RateMismatch
            );
        }
    }

    #[test]
    fn an_empty_deck_resolves_to_none() {
        let table = show(vec![row("strobe", "Strobe", Some(1))]);
        assert_eq!(
            resolve_deck(Some(&table), RATE, None, None, None, 0),
            DeckResolution::None
        );
    }

    #[test]
    fn a_deck_holding_the_previous_tracks_info_is_pending_not_matched() {
        let table = show(vec![row("strobe", "Strobe", Some(1))]);
        // Same id, but read off the previous DJ's stick on player 2.
        let previous = info(track_ref(2, 15), "Strobe", 300_000.0);
        assert_eq!(
            resolve(&table, Some(&previous), None, 0),
            DeckResolution::Pending
        );
        assert_eq!(resolve(&table, None, None, 0), DeckResolution::Pending);
    }

    #[test]
    fn a_deck_pending_past_the_timeout_is_unidentified_pending_timeout() {
        let table = show(vec![row("strobe", "Strobe", Some(1))]);
        assert_eq!(
            resolve(&table, None, None, PENDING_TIMEOUT_MS - 1),
            DeckResolution::Pending
        );
        assert_eq!(
            resolve(&table, None, None, PENDING_TIMEOUT_MS),
            fallback(UnidentifiedReason::PendingTimeout)
        );
    }

    #[test]
    fn a_pending_timeout_becomes_fallback_or_cut_per_the_switch() {
        let mut table = show(vec![row("strobe", "Strobe", Some(1))]);
        table.cut_when_unidentified = true;
        assert_eq!(
            resolve(&table, None, None, PENDING_TIMEOUT_MS),
            DeckResolution::Cut {
                reason: UnidentifiedReason::PendingTimeout
            }
        );
    }

    #[test]
    fn late_metadata_after_the_timeout_resolves_to_the_matched_row() {
        let table = show(vec![row("strobe", "Strobe", Some(1))]);
        let timed_out = resolve(&table, None, None, PENDING_TIMEOUT_MS);
        assert_eq!(
            resolve(&table, Some(&identified()), Some(&timed_out), 20_000),
            matched("strobe")
        );
    }

    #[test]
    fn a_zero_duration_track_is_unidentified_for_no_duration() {
        let table = show(vec![row("strobe", "Strobe", Some(1))]);
        let no_duration = info(loaded(), "Strobe", 0.0);
        assert_eq!(
            resolve(&table, Some(&no_duration), None, 0),
            fallback(UnidentifiedReason::NoDuration)
        );
    }

    #[test]
    fn one_matching_row_is_matched() {
        let table = show(vec![
            row("other", "Other", Some(2)),
            row("strobe", "STROBE", Some(1)),
        ]);
        // 301.4 s rounds to 301, within the 2 s tolerance of the row's 300.
        let near = info(loaded(), "Strobe", 301_400.0);
        assert_eq!(resolve(&table, Some(&near), None, 0), matched("strobe"));
    }

    #[test]
    fn no_row_matches_is_unidentified_no_match() {
        let table = show(vec![row("other", "Other", Some(2))]);
        assert_eq!(
            resolve(&table, Some(&identified()), None, 0),
            fallback(UnidentifiedReason::NoMatch)
        );
    }

    #[test]
    fn two_matching_rows_are_ambiguous_and_list_both_ids() {
        let table = show(vec![
            row("first", "Strobe", Some(1)),
            row("second", "Strobe", Some(2)),
        ]);
        assert_eq!(
            resolve(&table, Some(&identified()), None, 0),
            fallback(UnidentifiedReason::Ambiguous {
                row_ids: vec!["first".to_owned(), "second".to_owned()]
            })
        );
    }

    #[test]
    fn a_matched_row_with_a_blank_start_is_unidentified_blank_start() {
        let table = show(vec![row("strobe", "Strobe", None)]);
        assert_eq!(
            resolve(&table, Some(&identified()), None, 0),
            fallback(UnidentifiedReason::BlankStart {
                row_id: "strobe".to_owned()
            })
        );
    }

    #[test]
    fn typing_the_start_of_a_blank_row_puts_the_deck_on_it() {
        let blank = show(vec![row("strobe", "Strobe", None)]);
        let before = resolve(&blank, Some(&identified()), None, 0);

        let timed = show(vec![row("strobe", "Strobe", Some(1))]);
        assert_eq!(
            resolve(&timed, Some(&identified()), Some(&before), 0),
            matched("strobe")
        );
    }

    #[test]
    fn unidentified_becomes_cut_when_the_switch_is_on_and_fallback_otherwise() {
        let mut table = show(vec![row("other", "Other", Some(2))]);
        assert_eq!(
            resolve(&table, Some(&identified()), None, 0),
            fallback(UnidentifiedReason::NoMatch)
        );
        table.cut_when_unidentified = true;
        let cut = resolve(&table, Some(&identified()), None, 0);
        assert_eq!(
            cut,
            DeckResolution::Cut {
                reason: UnidentifiedReason::NoMatch
            }
        );
        // Flipping the switch back applies to a load already settled.
        table.cut_when_unidentified = false;
        assert_eq!(
            resolve(&table, Some(&identified()), Some(&cut), 0),
            fallback(UnidentifiedReason::NoMatch)
        );
    }

    #[test]
    fn a_cached_match_survives_an_edit_of_its_start_and_trim() {
        let table = show(vec![row("strobe", "Strobe", Some(1))]);
        let first = resolve(&table, Some(&identified()), None, 0);

        let mut edited_row = row("strobe", "Strobe", Some(5));
        edited_row.trim_ms = 80;
        // A second matching row arrives too: matched afresh, the deck would be ambiguous.
        let edited = show(vec![edited_row, row("copy", "Strobe", Some(7))]);
        let again = resolve(&edited, Some(&identified()), Some(&first), 0);
        assert_eq!(again, matched("strobe"));
        assert_eq!(
            timeline_for(&edited, &again),
            Timeline {
                start_frames: 5 * FRAMES_PER_HOUR,
                trim_ms: 80
            }
        );
    }

    #[test]
    fn a_cached_match_is_rematched_when_its_row_is_gone() {
        let before = show(vec![row("old-id", "Strobe", Some(1))]);
        let first = resolve(&before, Some(&identified()), None, 0);

        // Replace regenerates every id.
        let replaced = show(vec![row("new-id", "Strobe", Some(1))]);
        assert_eq!(
            resolve(&replaced, Some(&identified()), Some(&first), 0),
            matched("new-id")
        );
    }

    #[test]
    fn a_settled_unidentified_load_stays_so_until_another_track_loads() {
        let empty = show(vec![row("other", "Other", Some(2))]);
        let first = resolve(&empty, Some(&identified()), None, 0);

        // An import adds the loaded track's row: the outcome is fixed at load.
        let imported = show(vec![
            row("other", "Other", Some(2)),
            row("strobe", "Strobe", Some(1)),
        ]);
        assert_eq!(
            resolve(&imported, Some(&identified()), Some(&first), 0),
            fallback(UnidentifiedReason::NoMatch)
        );
        assert_eq!(
            resolve(&imported, Some(&identified()), None, 0),
            matched("strobe")
        );
    }

    #[test]
    fn the_matched_timeline_is_the_rows_and_the_fallback_has_no_trim() {
        let mut strobe = row("strobe", "Strobe", Some(1));
        strobe.trim_ms = -40;
        let table = show(vec![strobe]);

        assert_eq!(
            timeline_for(&table, &matched("strobe")),
            Timeline {
                start_frames: FRAMES_PER_HOUR,
                trim_ms: -40
            }
        );
        assert_eq!(
            timeline_for(&table, &fallback(UnidentifiedReason::NoMatch)),
            Timeline {
                start_frames: 23 * FRAMES_PER_HOUR,
                trim_ms: 0
            }
        );
        assert_eq!(
            timeline_for(&table, &DeckResolution::Pending),
            Timeline::default()
        );
    }
}
