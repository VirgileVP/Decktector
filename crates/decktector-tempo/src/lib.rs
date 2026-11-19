//! Tempo and phase output for lighting/VJ software.
//!
//! Reads every deck's BPM and beat position from the [`Engine`] and sends
//! them as OSC over UDP (grandMA3, MadMapper, Resolume and the like). What
//! goes where is the user's routing table ([`TempoOutputConfig`]): a list of
//! [`OscTarget`]s and a list of [`OscMessageConfig`]s, each one a source
//! (the master or a deck), a kind (BPM, bar start, each beat, or a custom
//! trigger and value), an OSC address and the targets that receive it.
//!
//! A message whose source has no value — no master, unknown BPM, deck
//! absent or not playing — sends nothing: receivers keep their last value.
//!
//! Phase resolution is bounded by the ~5 Hz deck status rate: a beat message
//! can lag the real beat by up to ~200 ms. That is a known V1 limit.
//!
//! The renderer never talks to this crate directly — the Tauri app shell
//! wraps [`TempoOutputService`] in commands and forwards
//! [`TempoOutputStatus`] events, the same bridge shape as the timecode layer.
//!
//! [`Engine`]: decktector_core::Engine

pub mod config;
pub mod osc;
mod output;
pub mod plan;
pub mod reading;
mod service;
pub mod status;
pub mod template;
pub mod validation;
pub mod value;

pub use config::{
    BpmRefresh, CustomValue, MessageKind, OscMessageConfig, OscTarget, TempoOutputConfig,
    TempoSource, Trigger, MAX_BPM_DECIMALS, MAX_DECK_ID,
};
pub use service::{TempoOutputService, FULL_REFRESH_PERIOD};
pub use status::{MessageIssue, TargetIssue, TempoOutputStatus};
pub use validation::ValidTempoConfig;
