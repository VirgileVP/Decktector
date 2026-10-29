//! The handle through which something outside the engine plays the manual
//! rig: it sends gestures in and watches the rig's state come out.

use std::sync::Arc;

use tokio::sync::{mpsc, watch};

use super::command::SimCommand;
use super::rig_state::SimRigState;
use crate::model::TrackInfo;

/// Cheap to clone; every clone drives the same rig.
#[derive(Clone)]
pub struct SimControl {
    commands: mpsc::UnboundedSender<SimCommand>,
    state: watch::Receiver<SimRigState>,
    library: Arc<[Arc<TrackInfo>]>,
}

impl SimControl {
    pub(super) fn new(
        commands: mpsc::UnboundedSender<SimCommand>,
        state: watch::Receiver<SimRigState>,
        library: Arc<[Arc<TrackInfo>]>,
    ) -> Self {
        Self {
            commands,
            state,
            library,
        }
    }

    /// Queues a gesture for the rig. Fails only when the engine that owns
    /// the rig has been dropped; a merely stopped engine applies it once
    /// restarted.
    pub fn send(&self, command: SimCommand) -> anyhow::Result<()> {
        self.commands
            .send(command)
            .map_err(|_| anyhow::anyhow!("the simulated rig is gone"))
    }

    /// The rig as of its last update.
    pub fn state(&self) -> SimRigState {
        self.state.borrow().clone()
    }

    /// Wakes on every update of the rig, about 25 times a second.
    pub fn subscribe(&self) -> watch::Receiver<SimRigState> {
        self.state.clone()
    }

    /// The full track at `index` of the sorted library — waveform and all,
    /// which [`SimRigState`] leaves out.
    pub fn track(&self, index: usize) -> Option<TrackInfo> {
        self.library.get(index).map(|track| (**track).clone())
    }
}
