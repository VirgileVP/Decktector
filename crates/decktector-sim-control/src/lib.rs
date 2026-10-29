//! Dev-only control surface for the hand-played simulated CDJs.
//!
//! A browser page (`sim-cdj.html`) renders four CDJs and sends the player's
//! gestures here as [`SimCommand`]s over a WebSocket; this crate forwards
//! them to the rig through a [`SimControl`] and streams the rig's state
//! back. It knows nothing about Tauri, the same shape as `decktector-remote`,
//! whose asset serving it reuses for the page itself.
//!
//! [`SimCommand`]: decktector_core::sim::SimCommand
//! [`SimControl`]: decktector_core::sim::SimControl

mod announcements;
mod audio;
pub mod server;
mod session;
pub mod wire;

pub use audio::AudioCatalog;
pub use server::{SimControlServer, SIM_CONTROL_PORT};
pub use wire::SimControlMessage;
