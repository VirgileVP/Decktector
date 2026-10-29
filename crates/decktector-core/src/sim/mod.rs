//! Scripted simulation of a Pro DJ Link network, for developing the UI
//! without hardware.
//!
//! Two modes share one [`SimEngine`]: [`SimMode::Auto`] scripts a DJ with
//! random behaviour; [`SimMode::Manual`] is a four-deck rig that only does
//! what a [`SimControl`] tells it to.

mod beat;
mod clock;
mod command;
mod control;
mod deck;
mod engine;
mod library;
mod manual;
mod rig;
mod rig_state;
mod tracks;

#[cfg(test)]
mod test_support;

pub use command::{SimCommand, TempoRange};
pub use control::SimControl;
pub use engine::{SimEngine, SimMode};
pub use library::{SimLibrary, SIM_TRACKS_ENV};
pub use rig_state::{SimDeckView, SimLoop, SimRigState, SimTrackSummary};
pub use tracks::{build_sim_track, SimTrackSpec, SIM_TRACKS};
