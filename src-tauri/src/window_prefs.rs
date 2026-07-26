//! Lets the renderer drive the window flags its own settings panel exposes.

use std::sync::Mutex;

use serde::Deserialize;

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowPrefs {
    pub always_on_top: bool,
    /// Keeps the display awake — a visualizer nobody touches for an hour
    /// must not let the screen blank in the middle of a set.
    pub prevent_sleep: bool,
}

/// Holds the sleep inhibitor for as long as the preference is on. Dropping
/// the guard is what releases it, so the `Option` is the on/off switch.
#[derive(Default)]
pub struct SleepBlocker(Mutex<Option<keepawake::KeepAwake>>);

impl SleepBlocker {
    pub fn set(&self, enabled: bool) -> anyhow::Result<()> {
        let mut guard = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        match (enabled, guard.is_some()) {
            (true, false) => {
                *guard = Some(
                    keepawake::Builder::default()
                        .display(true)
                        .reason("Decktective is visualizing a live set")
                        .app_name("Decktective")
                        .create()?,
                );
            }
            (false, true) => *guard = None,
            _ => {}
        }
        Ok(())
    }
}
