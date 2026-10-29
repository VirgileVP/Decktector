//! One connected control page: commands in, rig state and tracks out.

use axum::extract::ws::{Message, WebSocket};
use decktector_core::sim::{SimControl, SimRigState};

use crate::announcements::TrackAnnouncements;
use crate::audio::AudioCatalog;
use crate::wire::{parse_command, SimControlMessage};

/// The socket is gone; there is nobody left to serve.
struct Disconnected;

/// Serves one page until it disconnects or the rig goes away.
///
/// Every rig update is re-serialized whole, library summary included, for
/// each client. That is thousands of entries about 25 times a second, but it
/// is loopback traffic to a single dev page, so it stays simple rather than
/// diffing the library.
pub(crate) async fn run(mut socket: WebSocket, control: SimControl, audio: AudioCatalog) {
    let mut state_updates = control.subscribe();
    let mut announcements = TrackAnnouncements::default();

    let initial = state_updates.borrow_and_update().clone();
    if publish(&mut socket, &control, &audio, &mut announcements, initial)
        .await
        .is_err()
    {
        return;
    }

    loop {
        tokio::select! {
            incoming = socket.recv() => match incoming {
                None | Some(Ok(Message::Close(_))) => break,
                Some(Err(error)) => {
                    tracing::debug!(%error, "the sim control socket failed");
                    break;
                }
                Some(Ok(Message::Text(text))) => {
                    if !forward_command(&control, &text) {
                        break;
                    }
                }
                Some(Ok(Message::Binary(_))) => {
                    tracing::warn!("ignoring a binary frame on the sim control socket");
                }
                // axum answers pings itself.
                Some(Ok(Message::Ping(_) | Message::Pong(_))) => {}
            },
            changed = state_updates.changed() => {
                if changed.is_err() {
                    break;
                }
                let state = state_updates.borrow_and_update().clone();
                if publish(&mut socket, &control, &audio, &mut announcements, state).await.is_err() {
                    break;
                }
            }
        }
    }
}

/// Hands one text frame to the rig. Returns `false` when the rig is gone and
/// the connection should close; a malformed frame is logged and skipped.
fn forward_command(control: &SimControl, text: &str) -> bool {
    let command = match parse_command(text) {
        Ok(command) => command,
        Err(error) => {
            tracing::warn!(%error, frame = text, "ignoring a malformed sim control command");
            return true;
        }
    };
    match control.send(command) {
        Ok(()) => true,
        Err(error) => {
            tracing::error!(%error, "failed to send a command to the simulated rig");
            false
        }
    }
}

/// Sends the track messages the state makes necessary, then the state itself.
async fn publish(
    socket: &mut WebSocket,
    control: &SimControl,
    audio: &AudioCatalog,
    announcements: &mut TrackAnnouncements,
    state: SimRigState,
) -> Result<(), Disconnected> {
    for (deck_id, track_index) in announcements.changed(&state.decks) {
        let track = track_index.and_then(|index| control.track(index));
        let audio_url = track
            .as_ref()
            .and_then(|track| audio.url_for(track.track_id));
        let message = SimControlMessage::Track {
            deck_id,
            track,
            audio_url,
        };
        send(socket, &message).await?;
    }
    send(socket, &SimControlMessage::State { state }).await
}

/// A message that fails to serialize is logged and dropped; only a dead
/// socket is an error.
async fn send(socket: &mut WebSocket, message: &SimControlMessage) -> Result<(), Disconnected> {
    let text = match serde_json::to_string(message) {
        Ok(text) => text,
        Err(error) => {
            tracing::error!(%error, "failed to serialize a sim control message");
            return Ok(());
        }
    };
    socket
        .send(Message::Text(text.into()))
        .await
        .map_err(|_| Disconnected)
}
