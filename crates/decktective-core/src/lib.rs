//! Domain layer for Decktective: the normalized deck model, the engine
//! contract every backend implements, and the scripted simulation engine.
//!
//! Nothing here knows about Tauri, windows or the Pro DJ Link wire format —
//! the protocol lives in the `prolink` crate, the UI in the renderer.

pub mod engine;
pub mod model;
pub mod sim;
pub mod structure;
pub mod waveform;

pub use engine::{Engine, EngineEvent, EngineHandle};
