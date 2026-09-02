//! The real Pro DJ Link [`Engine`]: claims a virtual-player identity,
//! listens for status and beat traffic, reports live deck state through
//! [`EngineHandle`], and fetches track metadata over dbserver on every
//! track change (see [`crate::metadata`]).

use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use decktective_core::model::{DeckId, DeviceInfo, NetworkState};
use decktective_core::{Engine, EngineHandle};
use prolink::beat::{BeatMessage, BeatWatcher};
use prolink::discovery::{DeviceKind, DeviceWatcher, DiscoveredDevice};
use prolink::presence::Presence;
use prolink::status::{DeviceStatus, StatusWatcher};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

use crate::metadata::{FetchRequest, MetadataFetcher};
use crate::network;
use crate::normalize::to_deck_state;
use crate::position::PositionTracker;

/// Decktective's device name, as it appears in other Pro DJ Link tools'
/// device lists once presence has claimed a number.
const DEVICE_NAME: &str = "decktective";

/// How long to wait for [`DeviceWatcher`] to see at least one real peer
/// before picking a network interface — long enough to catch a keep-alive
/// or two (they arrive every ~1.5s) on a network that already has players
/// on it, short enough not to stall startup when there's only one adapter
/// anyway (the common case, resolved without needing a peer at all).
const INTERFACE_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(3);

/// Real Pro DJ Link backend. Shares its [`DeviceWatcher`] with the
/// diagnostics device panel (`src-tauri/src/devices.rs`) rather than owning
/// a second one — both just want the same "who's on the network" read.
pub struct ProlinkEngine {
    handle: Arc<EngineHandle>,
    device_watcher: Arc<DeviceWatcher>,
    tasks: Mutex<Vec<JoinHandle<()>>>,
    /// Set once per `start()` — `None` until then. A field rather than a
    /// local of `start()` because `stop()` needs to reach it too, to abort
    /// whatever per-track fetches are still in flight (each spawned as its
    /// own independent Tokio task, so aborting `tasks` above doesn't cascade
    /// to them).
    metadata_fetcher: Mutex<Option<Arc<MetadataFetcher>>>,
}

impl ProlinkEngine {
    pub fn new(device_watcher: Arc<DeviceWatcher>) -> Self {
        Self {
            handle: Arc::new(EngineHandle::new()),
            device_watcher,
            tasks: Mutex::new(Vec::new()),
            metadata_fetcher: Mutex::new(None),
        }
    }

    fn tasks_slot(&self) -> MutexGuard<'_, Vec<JoinHandle<()>>> {
        self.tasks
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[async_trait]
impl Engine for ProlinkEngine {
    async fn start(&self) -> anyhow::Result<()> {
        self.handle.set_network_state(NetworkState::Searching);

        let status_watcher = StatusWatcher::new();
        let beat_watcher = BeatWatcher::new();
        let presence = Presence::new(vcdj_id_override());

        // Resolved before the tasks lock is taken below: holding a
        // `std::sync::MutexGuard` across an `.await` would make this
        // function's future `!Send`, which the `Engine` trait object
        // requires (see `#[async_trait]` on the trait definition).
        let known_peer =
            wait_for_known_peer(&self.device_watcher, INTERFACE_DISCOVERY_TIMEOUT).await;
        let bind_ip = network::pick_interface(known_peer);
        // `MetadataFetcher` is built unconditionally, even if no interface
        // was resolved: without one, `presence` below never gets its `run`
        // task, so it never claims a number, and `MetadataFetcher`'s own
        // `device_number()` check already declines every fetch until one
        // exists — no separate degrade-gracefully path needed here.
        let metadata_fetcher = MetadataFetcher::new(
            Arc::clone(&presence),
            bind_ip
                .as_ref()
                .ok()
                .copied()
                .unwrap_or(Ipv4Addr::UNSPECIFIED),
        );
        *self
            .metadata_fetcher
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Arc::clone(&metadata_fetcher));

        let mut tasks = self.tasks_slot();
        tasks.push(tokio::spawn(Arc::clone(&status_watcher).run()));
        tasks.push(tokio::spawn(Arc::clone(&beat_watcher).run()));
        tasks.push(tokio::spawn(run_reporting(
            Arc::clone(&self.handle),
            Arc::clone(&self.device_watcher),
            status_watcher,
            beat_watcher,
            Arc::clone(&metadata_fetcher),
        )));

        match bind_ip {
            Ok(bind_ip) => {
                tasks.push(tokio::spawn(presence.run(
                    Arc::clone(&self.device_watcher),
                    DEVICE_NAME.to_string(),
                    bind_ip,
                )));
            }
            Err(error) => {
                // We still listen for and report real players' status/beat
                // traffic without a presence broadcast — just absent from
                // everyone else's device list, and from our own diagnostics
                // panel, and without metadata (which needs a claimed
                // number), until this is resolved.
                tracing::warn!(%error, "not broadcasting a Pro DJ Link presence this run");
            }
        }
        drop(tasks);

        self.handle.set_network_state(NetworkState::Connected);
        Ok(())
    }

    async fn stop(&self) -> anyhow::Result<()> {
        for task in self.tasks_slot().drain(..) {
            task.abort();
        }
        if let Some(fetcher) = self
            .metadata_fetcher
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
        {
            fetcher.abort_all();
        }
        self.handle.set_network_state(NetworkState::Offline);
        Ok(())
    }

    fn handle(&self) -> &EngineHandle {
        &self.handle
    }
}

/// The IP of the first discovered peer, waiting briefly if none is known
/// yet — used to pick which local network adapter shares the players'
/// subnet (see [`crate::network`]). Never fails outright: `None` just means
/// [`network::pick_interface`] falls back to its own default.
async fn wait_for_known_peer(
    device_watcher: &DeviceWatcher,
    timeout: Duration,
) -> Option<Ipv4Addr> {
    if let Some(ip) = first_peer_ip(&device_watcher.devices()) {
        return Some(ip);
    }

    let mut devices_rx = device_watcher.subscribe();
    let sleep = tokio::time::sleep(timeout);
    tokio::pin!(sleep);
    loop {
        tokio::select! {
            () = &mut sleep => return None,
            changed = devices_rx.changed() => {
                if changed.is_err() {
                    return None;
                }
                if let Some(ip) = first_peer_ip(&devices_rx.borrow_and_update()) {
                    return Some(ip);
                }
            }
        }
    }
}

fn first_peer_ip(devices: &[DiscoveredDevice]) -> Option<Ipv4Addr> {
    devices.iter().find_map(|device| device.ip.parse().ok())
}

/// Merges [`DeviceWatcher`]'s device list with the status/beat streams into
/// [`EngineHandle`] updates. One task, not three, so the position tracker
/// (status readings refine it, beat readings feed it) and the track-id
/// bookkeeping below need no locking — everything here runs sequentially
/// against local state. Metadata fetches themselves run on their own
/// spawned tasks (see [`MetadataFetcher::spawn_fetch`]) so a slow player
/// never blocks this loop from reporting everyone else's live status.
async fn run_reporting(
    handle: Arc<EngineHandle>,
    device_watcher: Arc<DeviceWatcher>,
    status_watcher: Arc<StatusWatcher>,
    beat_watcher: Arc<BeatWatcher>,
    metadata_fetcher: Arc<MetadataFetcher>,
) {
    let mut devices_rx = device_watcher.subscribe();
    let mut status_rx = status_watcher.subscribe();
    let mut beat_rx = beat_watcher.subscribe();
    let mut position = PositionTracker::new();
    let mut known_players: HashMap<DeckId, DeviceInfo> = HashMap::new();
    let mut known_track_ids: HashMap<DeckId, Option<u32>> = HashMap::new();

    // DeviceWatcher may already have been running a while (it's shared with
    // the diagnostics panel) — hydrate from what it already knows rather
    // than waiting for the next change.
    sync_devices(
        &handle,
        &metadata_fetcher,
        &mut known_players,
        &mut position,
        &device_watcher.devices(),
    );

    loop {
        tokio::select! {
            changed = devices_rx.changed() => {
                if changed.is_err() {
                    break; // The device watcher was dropped; nothing left to report.
                }
                let devices = devices_rx.borrow_and_update().clone();
                sync_devices(&handle, &metadata_fetcher, &mut known_players, &mut position, &devices);
            }
            status = status_rx.recv() => {
                match status {
                    Ok(status) => handle_status(&handle, &metadata_fetcher, &mut position, &known_players, &mut known_track_ids, status),
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            message = beat_rx.recv() => {
                match message {
                    Ok(BeatMessage::PrecisePosition(update)) => {
                        position.record_precise_position(update.device_number, update.position_ms, now_ms());
                    }
                    Ok(BeatMessage::Beat(_)) => {} // Not needed by this phase's DeckState mapping.
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_status(
    handle: &Arc<EngineHandle>,
    metadata_fetcher: &Arc<MetadataFetcher>,
    position: &mut PositionTracker,
    known_players: &HashMap<DeckId, DeviceInfo>,
    known_track_ids: &mut HashMap<DeckId, Option<u32>>,
    status: DeviceStatus,
) {
    if DeviceKind::from_device_number(status.device_number) != DeviceKind::Player {
        return; // The mixer shares this port but not this packet's shape.
    }
    let now = now_ms();
    let playhead_ms = position.estimate(status.device_number, &status, now);
    handle.update_deck_state(to_deck_state(&status, playhead_ms, now));

    let deck_id = status.device_number;
    if known_track_ids.get(&deck_id) == Some(&status.track_id) {
        return; // Same track as the last packet; nothing to fetch.
    }

    let Some(track_id) = status.track_id else {
        known_track_ids.insert(deck_id, None);
        metadata_fetcher.note_current_track(deck_id, None);
        handle.update_deck_track(deck_id, None);
        return;
    };

    // The media may live in another player entirely: on a linked booth every
    // deck plays off one stick, and asking a deck about a track it is only
    // streaming gets an empty answer (and, over NFS, a mount failure on an
    // empty slot).
    let source_device = if status.track_source_device == 0 {
        deck_id
    } else {
        status.track_source_device
    };
    let Some(player_ip) = known_players
        .get(&source_device)
        .and_then(|player| player.ip.parse().ok())
    else {
        // No address for that player yet. Status packets arrive every
        // ~200ms and keep-alives only every ~1.5s, so at startup this is
        // simply the normal order of events — and it is why the track id
        // is recorded *below*, once a fetch has actually been spawned:
        // marking it as handled here would make this deck's opening track
        // the one track that never gets metadata, until the DJ happens to
        // load another one.
        return;
    };

    known_track_ids.insert(deck_id, Some(track_id));
    metadata_fetcher.note_current_track(deck_id, Some(track_id));
    metadata_fetcher.spawn_fetch(
        Arc::clone(handle),
        FetchRequest {
            deck_id,
            source_device,
            player_ip,
            slot: status.media_slot,
            track_type: status.track_type,
            track_id,
            status_bpm: status.track_bpm,
        },
    );
}

/// Diffs `devices` against what was previously known and reports
/// connect/disconnect for the difference — [`DeviceWatcher`] only ever
/// hands back a full current snapshot, not a delta.
fn sync_devices(
    handle: &EngineHandle,
    metadata_fetcher: &Arc<MetadataFetcher>,
    known: &mut HashMap<DeckId, DeviceInfo>,
    position: &mut PositionTracker,
    devices: &[DiscoveredDevice],
) {
    let current: HashMap<DeckId, DeviceInfo> = devices
        .iter()
        .filter(|device| {
            device.kind == DeviceKind::Player && !prolink::presence::is_own_broadcast(device)
        })
        .map(|device| {
            (
                device.device_number,
                DeviceInfo {
                    deck_id: device.device_number,
                    name: device.name.clone(),
                    ip: device.ip.clone(),
                },
            )
        })
        .collect();

    for (&deck_id, info) in known.iter() {
        if !current.contains_key(&deck_id) {
            handle.device_disconnected(info.clone());
            position.forget(deck_id);
            metadata_fetcher.forget_device(deck_id);
        }
    }
    for (&deck_id, info) in &current {
        if !known.contains_key(&deck_id) {
            handle.device_connected(info.clone());
        }
    }
    *known = current;
}

/// `DECKTECTIVE_VCDJ_ID`: claim a specific virtual player number instead of
/// the default 5 — documented in the project's README as the escape hatch
/// for a full four-player network, where metadata queries (phase 3+) need a
/// number in 1-4 and one has been freed by renumbering a real player out of
/// the way. An unset or unparseable value means no override, the same as
/// before this env var existed.
fn vcdj_id_override() -> Option<u8> {
    std::env::var("DECKTECTIVE_VCDJ_ID")
        .ok()
        .and_then(|value| value.parse().ok())
}

fn now_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_millis() as f64)
        .unwrap_or_default()
}
