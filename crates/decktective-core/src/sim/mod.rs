//! Scripted simulation of a Pro DJ Link network, for developing the UI
//! without hardware.

mod engine;
mod tracks;

pub use engine::SimEngine;
pub use tracks::{build_sim_track, SimTrackSpec, SIM_TRACKS};
