//! The one place the simulation reads the wall clock.

use std::time::{SystemTime, UNIX_EPOCH};

/// Milliseconds since the Unix epoch — the `reported_at` anchor every
/// [`crate::model::DeckState`] carries so the renderer can interpolate.
pub(super) fn epoch_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_millis() as f64)
        .unwrap_or_default()
}
