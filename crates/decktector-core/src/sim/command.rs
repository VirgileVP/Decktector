//! What a person at a simulated CDJ rig can do.
//!
//! A [`SimCommand`] is one physical gesture — pressing a button, moving a
//! fader, touching a jog. The manual simulation (see [`super::rig::SimRig`])
//! applies them one at a time and has no other input, which is what makes it
//! deterministic. The JSON shape is the wire format of the control page, and
//! `ts-rs` writes its TypeScript twin next to the rest of the model.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::DeckId;

/// The pitch fader's travel, as the player's TEMPO RANGE button selects it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "TempoRange.ts")]
pub enum TempoRange {
    /// ±6 %
    Six,
    /// ±10 %, the factory setting of a CDJ-3000.
    #[default]
    Ten,
    /// ±16 %
    Sixteen,
    /// ±100 %
    Wide,
}

impl TempoRange {
    /// Pitch, in percent, with the fader at either end of its travel.
    pub fn percent(self) -> f64 {
        match self {
            Self::Six => 6.0,
            Self::Ten => 10.0,
            Self::Sixteen => 16.0,
            Self::Wide => 100.0,
        }
    }
}

/// One gesture on one deck (or one channel fader of the mixer).
///
/// Numeric inputs are clamped where they are applied, never rejected: a
/// control surface that overshoots should land on the end of the travel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(
    export,
    export_to = "SimCommand.ts",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SimCommand {
    /// Load a track of the sorted library; allowed at any time.
    Load {
        deck: DeckId,
        track_index: usize,
    },
    /// Empty the deck. A playing deck refuses, as a CDJ locks its eject
    /// button during playback.
    Eject {
        deck: DeckId,
    },
    PlayPause {
        deck: DeckId,
    },
    Cue {
        deck: DeckId,
    },
    Seek {
        deck: DeckId,
        position_ms: f64,
    },
    /// `position` runs from -1 to 1; +1 is the fastest end of the range.
    SetPitchFader {
        deck: DeckId,
        position: f64,
    },
    SetTempoRange {
        deck: DeckId,
        range: TempoRange,
    },
    /// The jog's top plate being touched (`true`) or released.
    JogTouch {
        deck: DeckId,
        touched: bool,
    },
    /// Platter movement while the top is touched, as a position delta.
    JogScratch {
        deck: DeckId,
        delta_ms: f64,
    },
    /// The jog's edge being dragged: -1 to 1, 0 releases it.
    JogNudge {
        deck: DeckId,
        bend: f64,
    },
    /// A hot cue pad, `"A"` to `"H"`.
    HotCue {
        deck: DeckId,
        button: String,
    },
    /// Start a loop of `beats` beats (4 or 8) at the playhead.
    Loop {
        deck: DeckId,
        beats: u8,
    },
    /// Leave an active loop, or return to the stored one.
    ReloopExit {
        deck: DeckId,
    },
    /// Toggle beat sync with the master.
    Sync {
        deck: DeckId,
    },
    /// Take the master role.
    Master {
        deck: DeckId,
    },
    /// The mixer's channel fader for `deck`, 0 (closed) to 1.
    SetChannelFader {
        deck: DeckId,
        level: f64,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_command_serialises_with_a_type_tag_and_camel_case_fields() {
        let json = serde_json::to_value(SimCommand::Load {
            deck: 2,
            track_index: 7,
        })
        .expect("serialises");

        assert_eq!(
            json,
            serde_json::json!({ "type": "load", "deck": 2, "trackIndex": 7 })
        );
    }

    #[test]
    fn a_command_reads_back_from_the_wire_format() {
        let command: SimCommand =
            serde_json::from_str(r#"{"type":"setTempoRange","deck":1,"range":"wide"}"#)
                .expect("parses");

        assert_eq!(
            command,
            SimCommand::SetTempoRange {
                deck: 1,
                range: TempoRange::Wide
            }
        );
    }

    #[test]
    fn a_command_of_an_unknown_type_is_rejected() {
        assert!(serde_json::from_str::<SimCommand>(r#"{"type":"explode","deck":1}"#).is_err());
    }

    #[test]
    fn each_tempo_range_spans_its_advertised_percentage() {
        assert_eq!(TempoRange::Six.percent(), 6.0);
        assert_eq!(TempoRange::Ten.percent(), 10.0);
        assert_eq!(TempoRange::Sixteen.percent(), 16.0);
        assert_eq!(TempoRange::Wide.percent(), 100.0);
    }
}
