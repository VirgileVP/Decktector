//! What the settings panel renders: whether the output runs and what is
//! wrong with the config or the network.

use decktector_core::model::DeckId;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A problem with one target: its config is unusable, or sending to it
/// failed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "TargetIssue.ts")]
pub struct TargetIssue {
    pub target_id: String,
    pub message: String,
}

/// A problem with one message's config; the message is not sent until the
/// user fixes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "MessageIssue.ts")]
pub struct MessageIssue {
    pub message_id: String,
    pub message: String,
}

/// What the settings panel renders, and what is actually being sent.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "TempoOutputStatus.ts")]
pub struct TempoOutputStatus {
    /// `true` when the output is enabled and at least one usable message
    /// has a usable target.
    pub running: bool,
    pub master_deck_id: Option<DeckId>,
    /// The master's BPM, `None` while there is no playing master or its BPM
    /// is unknown.
    pub master_bpm: Option<f64>,
    /// Unusable targets (invalid host or port) and targets the last send to
    /// failed for.
    pub target_issues: Vec<TargetIssue>,
    /// Unusable messages.
    pub message_issues: Vec<MessageIssue>,
}
