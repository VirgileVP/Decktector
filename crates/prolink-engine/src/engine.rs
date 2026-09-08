//! The real Pro DJ Link [`Engine`]: claims a virtual-player identity,
//! listens for status and beat traffic, reports live deck state through
//! [`EngineHandle`], and fetches track metadata over dbserver on every
//! track change (see [`crate::metadata`]).

use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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
    /// Active `Presence::run` task, if presence is currently broadcasting.
    /// Spawned dynamically as peers arrive or if the active interface changes.
    presence_task: Arc<Mutex<Option<JoinHandle<()>>>>,
}

impl ProlinkEngine {
    pub fn new(device_watcher: Arc<DeviceWatcher>) -> Self {
        Self {
            handle: Arc::new(EngineHandle::new()),
            device_watcher,
            tasks: Mutex::new(Vec::new()),
            metadata_fetcher: Mutex::new(None),
            presence_task: Arc::new(Mutex::new(None)),
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

        let metadata_fetcher = MetadataFetcher::new(
            Arc::clone(&presence),
            Ipv4Addr::UNSPECIFIED,
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
            presence,
            Arc::clone(&self.presence_task),
        )));
        drop(tasks);

        Ok(())
    }

    async fn stop(&self) -> anyhow::Result<()> {
        for task in self.tasks_slot().drain(..) {
            task.abort();
        }
        if let Some(presence_task) = self
            .presence_task
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
        {
            presence_task.abort();
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

fn first_peer_ip(devices: &[DiscoveredDevice]) -> Option<Ipv4Addr> {
    devices
        .iter()
        .filter(|device| !prolink::presence::is_own_broadcast(device))
        .find_map(|device| device.ip.parse().ok())
}

/// Dynamically resolves the network interface matching any currently
/// discovered peer, binding presence keep-alives and metadata fetching to it.
/// Sets [`NetworkState::Connected`] if peers exist, or [`NetworkState::Searching`]
/// if none are currently present on the network.
fn sync_network_and_presence(
    handle: &EngineHandle,
    device_watcher: &Arc<DeviceWatcher>,
    presence: &Arc<Presence>,
    metadata_fetcher: &Arc<MetadataFetcher>,
    presence_task_slot: &Arc<Mutex<Option<JoinHandle<()>>>>,
    current_bind_ip: &mut Option<Ipv4Addr>,
    devices: &[DiscoveredDevice],
) {
    let peer_ip = first_peer_ip(devices);
    if let Some(peer_ip) = peer_ip {
        match network::pick_interface(Some(peer_ip)) {
            Ok(new_bind_ip) => {
                if current_bind_ip.as_ref() != Some(&new_bind_ip) {
                    tracing::info!(
                        %peer_ip,
                        %new_bind_ip,
                        "discovered peer; binding presence and metadata fetcher to interface"
                    );
                    if let Some(old_task) = presence_task_slot
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .take()
                    {
                        old_task.abort();
                    }
                    metadata_fetcher.set_bind_ip(new_bind_ip);
                    let new_task = tokio::spawn(Arc::clone(presence).run(
                        Arc::clone(device_watcher),
                        DEVICE_NAME.to_string(),
                        new_bind_ip,
                    ));
                    *presence_task_slot
                        .lock()
                        .unwrap_or_else(|p| p.into_inner()) = Some(new_task);
                    *current_bind_ip = Some(new_bind_ip);
                }
                handle.set_network_state(NetworkState::Connected);
            }
            Err(error) => {
                tracing::warn!(%error, %peer_ip, "failed to resolve local network interface for peer");
            }
        }
    } else {
        handle.set_network_state(NetworkState::Searching);
    }
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
    presence: Arc<Presence>,
    presence_task_slot: Arc<Mutex<Option<JoinHandle<()>>>>,
) {
    let mut devices_rx = device_watcher.subscribe();
    let mut status_rx = status_watcher.subscribe();
    let mut beat_rx = beat_watcher.subscribe();
    let mut position = PositionTracker::new();
    let mut known_players: HashMap<DeckId, DeviceInfo> = HashMap::new();
    let mut known_track_ids: HashMap<DeckId, Option<u32>> = HashMap::new();
    let mut fetch_cooldown: HashMap<DeckId, Instant> = HashMap::new();
    let mut current_bind_ip: Option<Ipv4Addr> = None;

    // DeviceWatcher may already have been running a while (it's shared with
    // the diagnostics panel) — hydrate from what it already knows rather
    // than waiting for the next change.
    let initial_devices = device_watcher.devices();
    sync_network_and_presence(
        &handle,
        &device_watcher,
        &presence,
        &metadata_fetcher,
        &presence_task_slot,
        &mut current_bind_ip,
        &initial_devices,
    );
    sync_devices(
        &handle,
        &metadata_fetcher,
        &mut known_players,
        &mut position,
        &mut known_track_ids,
        &mut fetch_cooldown,
        &initial_devices,
    );

    loop {
        tokio::select! {
            changed = devices_rx.changed() => {
                if changed.is_err() {
                    break; // The device watcher was dropped; nothing left to report.
                }
                let devices = devices_rx.borrow_and_update().clone();
                sync_network_and_presence(
                    &handle,
                    &device_watcher,
                    &presence,
                    &metadata_fetcher,
                    &presence_task_slot,
                    &mut current_bind_ip,
                    &devices,
                );
                sync_devices(
                    &handle,
                    &metadata_fetcher,
                    &mut known_players,
                    &mut position,
                    &mut known_track_ids,
                    &mut fetch_cooldown,
                    &devices,
                );
            }
            status = status_rx.recv() => {
                match status {
                    Ok(status) => handle_status(
                        &handle,
                        &metadata_fetcher,
                        &mut position,
                        &known_players,
                        &mut known_track_ids,
                        &mut fetch_cooldown,
                        status,
                    ),
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

    if let Some(task) = presence_task_slot
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .take()
    {
        task.abort();
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_status(
    handle: &Arc<EngineHandle>,
    metadata_fetcher: &Arc<MetadataFetcher>,
    position: &mut PositionTracker,
    known_players: &HashMap<DeckId, DeviceInfo>,
    known_track_ids: &mut HashMap<DeckId, Option<u32>>,
    fetch_cooldown: &mut HashMap<DeckId, Instant>,
    status: DeviceStatus,
) {
    if DeviceKind::from_device_number(status.device_number) != DeviceKind::Player {
        return; // The mixer shares this port but not this packet's shape.
    }
    let now = now_ms();
    let playhead_ms = position.estimate(status.device_number, &status, now);
    handle.update_deck_state(to_deck_state(&status, playhead_ms, now));

    let deck_id = status.device_number;

    // Reset backoff if the track ID reported by the player changed
    if known_track_ids.get(&deck_id) != Some(&status.track_id) {
        fetch_cooldown.remove(&deck_id);
    }

    if let Some(track_id) = status.track_id {
        if handle.track_id(deck_id) == Some(track_id) {
            return; // Exact track metadata already loaded and present in handle; nothing to fetch.
        }
    } else if known_track_ids.get(&deck_id) == Some(&None) {
        return; // Already known to have no track loaded.
    }

    if metadata_fetcher.is_fetching(deck_id) {
        return; // Fetch currently in progress; avoid duplicate parallel requests.
    }

    if let Some(&attempted_at) = fetch_cooldown.get(&deck_id) {
        if attempted_at.elapsed() < Duration::from_secs(3) {
            return; // Back off briefly after an attempt before retrying.
        }
    }

    let Some(track_id) = status.track_id else {
        known_track_ids.insert(deck_id, None);
        fetch_cooldown.remove(&deck_id);
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
    fetch_cooldown.insert(deck_id, Instant::now());
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
    known_track_ids: &mut HashMap<DeckId, Option<u32>>,
    fetch_cooldown: &mut HashMap<DeckId, Instant>,
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
            known_track_ids.remove(&deck_id);
            fetch_cooldown.remove(&deck_id);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_device(number: u8, ip: &str, mac: &str) -> DiscoveredDevice {
        DiscoveredDevice {
            name: "CDJ-3000".to_string(),
            device_number: number,
            kind: DeviceKind::Player,
            ip: ip.to_string(),
            mac_address: mac.to_string(),
            peer_count: 1,
        }
    }

    #[test]
    fn first_peer_ip_finds_valid_ip_and_skips_own_broadcast() {
        let own = test_device(5, "192.168.1.50", "02:00:00:00:00:05");
        let cdj = test_device(1, "192.168.1.10", "aa:bb:cc:dd:ee:01");

        assert_eq!(first_peer_ip(&[own]), None);
        assert_eq!(
            first_peer_ip(&[cdj]),
            Some("192.168.1.10".parse().unwrap())
        );
    }

    #[tokio::test]
    async fn sync_network_transitions_searching_to_connected_and_back() {
        let handle = EngineHandle::new();
        let device_watcher = DeviceWatcher::new();
        let presence = Presence::new(None);
        let metadata_fetcher = MetadataFetcher::new(Arc::clone(&presence), Ipv4Addr::UNSPECIFIED);
        let presence_task_slot = Arc::new(Mutex::new(None));
        let mut current_bind_ip = None;

        // Initially no devices: state is Searching
        sync_network_and_presence(
            &handle,
            &device_watcher,
            &presence,
            &metadata_fetcher,
            &presence_task_slot,
            &mut current_bind_ip,
            &[],
        );
        assert_eq!(handle.snapshot().network_state, NetworkState::Searching);
        assert_eq!(current_bind_ip, None);

        // Discovered peer on loopback/local: transitions to Connected
        let peer = test_device(1, "127.0.0.1", "aa:bb:cc:dd:ee:01");
        sync_network_and_presence(
            &handle,
            &device_watcher,
            &presence,
            &metadata_fetcher,
            &presence_task_slot,
            &mut current_bind_ip,
            &[peer],
        );
        assert_eq!(handle.snapshot().network_state, NetworkState::Connected);
        assert!(current_bind_ip.is_some());
        assert_eq!(metadata_fetcher.bind_ip(), current_bind_ip.unwrap());

        // Devices disappear: state transitions back to Searching
        sync_network_and_presence(
            &handle,
            &device_watcher,
            &presence,
            &metadata_fetcher,
            &presence_task_slot,
            &mut current_bind_ip,
            &[],
        );
        assert_eq!(handle.snapshot().network_state, NetworkState::Searching);

        // Cleanup presence task if running
        if let Some(task) = presence_task_slot.lock().unwrap().take() {
            task.abort();
        };
    }

    #[tokio::test]
    async fn prolink_engine_starts_immediately_in_searching_state() {
        let device_watcher = DeviceWatcher::new();
        let engine = ProlinkEngine::new(device_watcher);
        engine.start().await.expect("start should succeed");

        assert_eq!(
            engine.handle().snapshot().network_state,
            NetworkState::Searching
        );

        engine.stop().await.expect("stop should succeed");
        assert_eq!(
            engine.handle().snapshot().network_state,
            NetworkState::Offline
        );
    }

    #[test]
    fn sync_devices_disconnect_clears_known_track_ids_and_cooldown() {
        let handle = EngineHandle::new();
        let presence = Presence::new(None);
        let metadata_fetcher = MetadataFetcher::new(presence, Ipv4Addr::LOCALHOST);
        let mut known_players = HashMap::new();
        let mut position = PositionTracker::new();
        let mut known_track_ids = HashMap::new();
        let mut fetch_cooldown = HashMap::new();

        let peer = test_device(2, "192.168.1.20", "aa:bb:cc:dd:ee:02");
        sync_devices(
            &handle,
            &metadata_fetcher,
            &mut known_players,
            &mut position,
            &mut known_track_ids,
            &mut fetch_cooldown,
            &[peer],
        );

        known_track_ids.insert(2, Some(42));
        fetch_cooldown.insert(2, Instant::now());

        // Peer disappears:
        sync_devices(
            &handle,
            &metadata_fetcher,
            &mut known_players,
            &mut position,
            &mut known_track_ids,
            &mut fetch_cooldown,
            &[],
        );

        assert!(!known_track_ids.contains_key(&2));
        assert!(!fetch_cooldown.contains_key(&2));
    }
}
