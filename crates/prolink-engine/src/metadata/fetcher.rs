//! Fetches one track's metadata and reports it through [`EngineHandle`],
//! with the timeout/backoff design [`crate::engine::run_reporting`]'s
//! status handling relies on to stay responsive when a player never
//! answers.
//!
//! Two routes, tried in order, mirroring the pre-Rust engine's own
//! `TrackLoader.load()` priority: [`super::nfs`] first (needs no virtual
//! player number, gets everything including cues and artwork), falling
//! back to dbserver (needs a virtual player number in 1-4, but is fully
//! protocol-documented and fake-server-tested) only if NFS didn't produce
//! a track. Each route has its own failure cache — a device failing on one
//! must never block the other, since they're independent transports with
//! independent reasons to fail (no NFS export on this media vs. our
//! virtual number being out of dbserver's accepted range, say).
//!
//! Timeouts and the retry backoff are ported from decktective's pre-Rust
//! engine's `track-loader.ts`, whose values came from real four-CDJ-3000
//! testing: `METADATA_TIMEOUT`, `ASSET_TIMEOUT`, `CONNECT_TIMEOUT`,
//! `RETRY_FAILED_SOURCE_AFTER`. `NFS_CONNECT_TIMEOUT`/`NFS_FILE_TIMEOUT` are
//! this phase's own judgment (crate-digger's reference implementation only
//! tunes the *inner* per-RPC-call retransmit loop, not an outer ceiling on
//! a whole multi-call sequence) — reasonable starting points, not
//! hardware-validated.

use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use decktective_core::model::{DeckId, TrackInfo};
use decktective_core::structure::{self, StructureInput};
use decktective_core::EngineHandle;
use prolink::dbserver::{beatgrid, waveform as db_waveform, Connection};
use prolink::presence::Presence;
use prolink::status::MediaSlot;
use tokio::sync::Mutex as AsyncMutex;
use tokio::task::JoinHandle;
use tokio::time::timeout;

use super::{nfs, normalize};

const METADATA_TIMEOUT: Duration = Duration::from_secs(30);
const ASSET_TIMEOUT: Duration = Duration::from_secs(15);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Covers the NFS route's whole portmap+mount+lookup-chain sequence in
/// [`prolink::nfs::NfsClient::open`] — each individual RPC call inside it
/// is separately bounded by its own 250ms-retransmit/4-retry loop; this is
/// a ceiling on top of that, not instead of it (mirrors `CONNECT_TIMEOUT`'s
/// relationship to dbserver's TCP connect).
const NFS_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// Covers one whole file download (`export.pdb`, or a track's `.EXT`) —
/// smaller than dbserver's `METADATA_TIMEOUT` since NFS has no equivalent
/// of the menu-render round-trip overhead, but generous for a large
/// library's database file over 2048-byte NFS reads.
const NFS_FILE_TIMEOUT: Duration = Duration::from_secs(20);
const RETRY_FAILED_SOURCE_AFTER: Duration = Duration::from_secs(60);

/// What one status packet supplies about the track it names — everything
/// [`MetadataFetcher::spawn_fetch`] needs beyond what it already tracks
/// itself (the pooled dbserver connection, the failure caches).
pub struct FetchRequest {
    pub deck_id: DeckId,
    /// The player whose media holds the track — the deck itself on a
    /// stand-alone player, a *different* player on a linked booth (see
    /// `prolink::status::DeviceStatus::track_source_device`). Both the
    /// address below and the device number every query names have to be
    /// this player's, not the deck's.
    pub source_device: u8,
    pub player_ip: Ipv4Addr,
    pub slot: MediaSlot,
    pub track_type: u8,
    pub track_id: u32,
    /// The status packet's own BPM reading, used if neither metadata route
    /// carries one (see `metadata::normalize::bpm`).
    pub status_bpm: Option<f64>,
}

/// Orchestrates both metadata routes across every deck: a pooled, mutex
/// -serialized dbserver connection per device (matching "a remote database
/// connection is a single socket, serialized behind one lock" from the
/// pre-Rust engine — NFS has no equivalent pool, see this module's doc
/// comment on why), independent per-route failure caches, and enough
/// bookkeeping to detect a fetch being superseded by a later track change
/// before it reports a stale result.
pub struct MetadataFetcher {
    presence: Arc<Presence>,
    bind_ip: Ipv4Addr,
    /// Pooled dbserver connections, keyed by the player whose media is
    /// being read — [`FetchRequest::source_device`], never [`DeckId`].
    /// A deck-keyed pool is wrong on exactly the network this whole module
    /// exists for: on a linked booth a deck plays off *another* player's
    /// stick, so the first fetch pools a socket to that player, and the
    /// next track from a different stick reuses it — a live connection to
    /// the wrong host, carrying a query that correctly names the right
    /// device number, which the player answers with an empty menu. No
    /// timeout, no error, just silently missing metadata.
    ///
    /// Seen on the 2026-08-21 booth capture: deck 3 played off player 4's
    /// USB all night and kept asking player 3 about it, because an earlier
    /// track off player 3's own stick had pooled that connection first.
    ///
    /// The address is half the key so a pooled socket can never be handed
    /// out for a host it was not opened to, whatever renumbering happens
    /// on the network underneath.
    connections: Mutex<HashMap<(u8, Ipv4Addr), Arc<AsyncMutex<Connection>>>>,
    /// Per-route failure records, keyed by the media's player for the same
    /// reason as [`Self::connections`]: a stick with no NFS export must not
    /// stop the deck reading its *next* track off a stick that has one,
    /// and a deck is not what either route talks to.
    nfs_failures: Mutex<HashMap<u8, Instant>>,
    dbserver_failures: Mutex<HashMap<u8, Instant>>,
    /// The track id `crate::engine::run_reporting` most recently saw for
    /// each deck — updated by [`Self::note_current_track`] from that same
    /// single-threaded task, read by a spawned fetch task after its network
    /// work completes to decide whether its result is still relevant.
    /// Necessarily duplicates `run_reporting`'s own local map: that map
    /// can't be touched from a spawned task without adding a lock to
    /// `run_reporting`'s hot path, which its own design deliberately
    /// avoids.
    current_track: Mutex<HashMap<DeckId, Option<u32>>>,
    tasks: Mutex<Vec<JoinHandle<()>>>,
}

impl MetadataFetcher {
    pub fn new(presence: Arc<Presence>, bind_ip: Ipv4Addr) -> Arc<Self> {
        Arc::new(Self {
            presence,
            bind_ip,
            connections: Mutex::new(HashMap::new()),
            nfs_failures: Mutex::new(HashMap::new()),
            dbserver_failures: Mutex::new(HashMap::new()),
            current_track: Mutex::new(HashMap::new()),
            tasks: Mutex::new(Vec::new()),
        })
    }

    pub fn note_current_track(&self, deck_id: DeckId, track_id: Option<u32>) {
        self.lock(&self.current_track).insert(deck_id, track_id);
    }

    /// Drops everything held for a device that just left the network: its
    /// pooled dbserver connections, both routes' failure records, and — a
    /// departing player being a deck as well as a possible media source —
    /// the track id tracked for the deck of the same number. Nothing stale
    /// must linger for whatever player is later assigned that number.
    ///
    /// The connection sweep is a `retain` rather than a `remove` because
    /// the pool is keyed by device *and* address: one device can hold more
    /// than one entry if it reappeared on a new address.
    pub fn forget_device(&self, device: u8) {
        self.lock(&self.connections).retain(|(pooled_device, _), _| *pooled_device != device);
        self.lock(&self.nfs_failures).remove(&device);
        self.lock(&self.dbserver_failures).remove(&device);
        self.lock(&self.current_track).remove(&device);
    }

    /// Spawns the fetch in the background — never blocks the caller
    /// (`crate::engine::run_reporting`'s status-handling path).
    pub fn spawn_fetch(self: &Arc<Self>, handle: Arc<EngineHandle>, request: FetchRequest) {
        let fetcher = Arc::clone(self);
        let task = tokio::spawn(async move {
            fetcher.fetch_and_report(handle, request).await;
        });

        let mut tasks = self.lock(&self.tasks);
        tasks.retain(|task| !task.is_finished());
        tasks.push(task);
    }

    pub fn abort_all(&self) {
        for task in self.lock(&self.tasks).drain(..) {
            task.abort();
        }
    }

    fn lock<'a, T>(&self, mutex: &'a Mutex<T>) -> MutexGuard<'a, T> {
        mutex.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// `source_device` is the player holding the media, not the deck — see
    /// [`Self::nfs_failures`].
    fn is_failing(&self, failures: &Mutex<HashMap<u8, Instant>>, source_device: u8) -> bool {
        let mut failures = self.lock(failures);
        match failures.get(&source_device) {
            Some(&failed_at) if failed_at.elapsed() < RETRY_FAILED_SOURCE_AFTER => true,
            Some(_) => {
                failures.remove(&source_device);
                false
            }
            None => false,
        }
    }

    fn mark_failed(&self, failures: &Mutex<HashMap<u8, Instant>>, source_device: u8) {
        self.lock(failures).insert(source_device, Instant::now());
    }

    fn is_superseded(&self, deck_id: DeckId, track_id: u32) -> bool {
        self.lock(&self.current_track).get(&deck_id) != Some(&Some(track_id))
    }

    async fn fetch_and_report(self: Arc<Self>, handle: Arc<EngineHandle>, request: FetchRequest) {
        let FetchRequest { deck_id, source_device, player_ip, slot, track_type, track_id, status_bpm } = request;

        // Neither route takes `deck_id`: everything they key on — the
        // connection pool, both failure caches, every query field — belongs
        // to the player holding the media. Keeping the deck out of scope
        // there is what stops the two `u8`s being swapped again.
        let track_info = match self.try_nfs(source_device, player_ip, slot, track_id, status_bpm).await {
            Some(track_info) => Some(track_info),
            None => self.try_dbserver(source_device, player_ip, slot, track_type, track_id, status_bpm).await,
        };

        let Some(track_info) = track_info else {
            return; // Both routes failed or had nothing for this track — leave the deck's last-known metadata as is.
        };
        if self.is_superseded(deck_id, track_id) {
            return;
        }
        handle.update_deck_track(deck_id, Some(track_info));
    }

    /// The NFS route — see [`super::nfs`]. No connection pooling (unlike
    /// dbserver below): every call currently re-opens and re-downloads
    /// `export.pdb`, a known, accepted inefficiency for this pass rather
    /// than added caching complexity before the basic flow is even
    /// hardware-validated — see this module's doc comment.
    async fn try_nfs(&self, source_device: u8, player_ip: Ipv4Addr, slot: MediaSlot, track_id: u32, status_bpm: Option<f64>) -> Option<TrackInfo> {
        if self.is_failing(&self.nfs_failures, source_device) {
            return None;
        }
        let fetch = nfs::fetch_track_info(player_ip, self.bind_ip, slot, track_id, status_bpm);
        match timeout(NFS_CONNECT_TIMEOUT + NFS_FILE_TIMEOUT, fetch).await {
            Ok(Ok(Some(track_info))) => Some(track_info),
            Ok(Ok(None)) => None, // Connected fine; this track just isn't in this media's database.
            Ok(Err(error)) => {
                tracing::debug!(source_device, %error, "NFS metadata fetch failed");
                self.mark_failed(&self.nfs_failures, source_device);
                None
            }
            Err(_) => {
                tracing::debug!(source_device, "NFS metadata fetch timed out");
                self.mark_failed(&self.nfs_failures, source_device);
                None
            }
        }
    }

    async fn try_dbserver(
        &self,
        source_device: u8,
        player_ip: Ipv4Addr,
        slot: MediaSlot,
        track_type: u8,
        track_id: u32,
        status_bpm: Option<f64>,
    ) -> Option<TrackInfo> {
        if self.is_failing(&self.dbserver_failures, source_device) {
            return None;
        }
        let connection = self.connection_for(source_device, player_ip).await?;
        let slot = slot.to_byte();

        let mut connection = connection.lock().await;
        // Only a gate: our own number identified us once, in the setup
        // message `Connection::open` sent. Losing it mid-fetch means the
        // connection no longer speaks for anyone. Every *query* below names
        // the player being asked about instead — passing our own number
        // there gets an empty menu back from real hardware.
        self.presence.device_number()?;

        let items = match timeout(METADATA_TIMEOUT, connection.fetch_metadata(source_device, slot, track_type, track_id)).await {
            Ok(Ok(items)) if !items.is_empty() => items,
            Ok(Ok(_)) => return None, // The player answered: this track has no metadata.
            _ => {
                self.mark_failed(&self.dbserver_failures, source_device);
                self.lock(&self.connections).remove(&(source_device, player_ip));
                return None;
            }
        };

        let beat_grid_blob = timeout(ASSET_TIMEOUT, connection.fetch_beat_grid(source_device, slot, track_type, track_id))
            .await
            .ok()
            .and_then(Result::ok)
            .flatten();
        let waveform_blob = timeout(
            ASSET_TIMEOUT,
            connection.fetch_anlz_tag(
                source_device,
                slot,
                track_type,
                track_id,
                db_waveform::COLOR_WAVEFORM_DETAIL_TAG,
                db_waveform::COLOR_WAVEFORM_FILE_EXTENSION,
            ),
        )
        .await
        .ok()
        .and_then(Result::ok)
        .flatten();

        Some(assemble_dbserver_track_info(track_id, &items, status_bpm, beat_grid_blob.as_deref(), waveform_blob.as_deref()))
    }

    /// The pooled dbserver connection to the player holding the media,
    /// opening a fresh one (bounded by [`CONNECT_TIMEOUT`]) if there isn't
    /// one yet. Keyed by device *and* address — see [`Self::connections`]
    /// for why the deck is not part of this.
    async fn connection_for(&self, source_device: u8, player_ip: Ipv4Addr) -> Option<Arc<AsyncMutex<Connection>>> {
        let key = (source_device, player_ip);
        if let Some(existing) = self.lock(&self.connections).get(&key).cloned() {
            return Some(existing);
        }

        let our_number = self.presence.device_number()?;
        let opened = timeout(CONNECT_TIMEOUT, Connection::open(player_ip, self.bind_ip, our_number)).await;
        match opened {
            Ok(Ok(connection)) => {
                let wrapped = Arc::new(AsyncMutex::new(connection));
                self.lock(&self.connections).insert(key, Arc::clone(&wrapped));
                Some(wrapped)
            }
            Ok(Err(error)) => {
                tracing::debug!(source_device, %player_ip, %error, "failed to open a dbserver connection");
                None
            }
            Err(_) => {
                tracing::debug!(source_device, %player_ip, "timed out opening a dbserver connection");
                None
            }
        }
    }
}

fn assemble_dbserver_track_info(
    track_id: u32,
    items: &[prolink::dbserver::client::TrackMenuItem],
    status_bpm: Option<f64>,
    beat_grid_blob: Option<&[u8]>,
    waveform_blob: Option<&[u8]>,
) -> TrackInfo {
    let beat_grid = beat_grid_blob.map(|blob| normalize::beat_grid(&beatgrid::parse_beat_grid(blob)));
    let waveform = waveform_blob.and_then(|blob| normalize::waveform_data(&db_waveform::parse_color_waveform_detail(blob)));
    let duration_ms = normalize::duration_ms(items);
    let cues = Vec::new(); // Cue-list wire format not yet confirmed — see prolink::dbserver::cues.

    // Borrowed here, before the same values move into the TrackInfo literal
    // below — mirrors decktective_core::sim::tracks::build_sim_track's
    // sequencing exactly.
    let events = structure::analyze(&StructureInput {
        waveform: waveform.as_ref(),
        beat_grid: beat_grid.as_deref(),
        duration_ms,
        cues: &cues,
    });

    TrackInfo {
        track_id,
        title: normalize::title(items).unwrap_or_default(),
        artist: normalize::artist(items),
        album: normalize::album(items),
        genre: normalize::genre(items),
        key: normalize::key(items),
        label: normalize::label(items),
        comment: normalize::comment(items),
        duration_ms,
        bpm: normalize::bpm(items, status_bpm),
        // Artwork needs the album-art id captured from the track-title menu
        // item's auxiliary numeric field, which — unlike the fields this
        // parser reads with confidence (title/artist/album/genre/key/label
        // /comment/duration/tempo, each keyed by a confirmed item_type) —
        // isn't pinned down precisely enough yet (see
        // `prolink::dbserver::client::TrackMenuItem`'s doc comment on why
        // its `number` field is a best-effort scan, not a fixed offset).
        // Deferred rather than risking fetching the wrong image — the NFS
        // route (tried first) already covers artwork properly.
        artwork: None,
        beat_grid,
        waveform,
        cues,
        events,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prolink::dbserver::client::TrackMenuItem;
    use prolink::dbserver::queries::MenuItemType;

    fn menu_item(item_type: MenuItemType, text: &str) -> TrackMenuItem {
        TrackMenuItem { item_type, text: Some(text.to_string()), number: None }
    }

    #[test]
    fn assembles_a_track_info_from_menu_items_alone() {
        let items = vec![menu_item(MenuItemType::TrackTitle, "Strobe"), menu_item(MenuItemType::Artist, "deadmau5")];
        let track = assemble_dbserver_track_info(4207, &items, Some(128.0), None, None);

        assert_eq!(track.track_id, 4207);
        assert_eq!(track.title, "Strobe");
        assert_eq!(track.artist.as_deref(), Some("deadmau5"));
        assert_eq!(track.bpm, Some(128.0));
        assert!(track.beat_grid.is_none());
        assert!(track.waveform.is_none());
        assert!(track.events.is_empty(), "no beat grid/waveform means nothing to analyse");
    }

    #[test]
    fn a_missing_title_item_falls_back_to_an_empty_title_rather_than_panicking() {
        let track = assemble_dbserver_track_info(1, &[], None, None, None);
        assert_eq!(track.title, "");
    }

    fn fetcher() -> Arc<MetadataFetcher> {
        MetadataFetcher::new(Presence::new(None), Ipv4Addr::LOCALHOST)
    }

    /// The `b5cbf4f`-class regression, at the orchestration level: a device
    /// that just failed to connect is skipped (not re-attempted) on the
    /// very next track change, so one bad player doesn't spend a fresh
    /// timeout on every subsequent track it loads.
    #[test]
    fn a_freshly_failed_device_is_skipped() {
        let fetcher = fetcher();
        fetcher.mark_failed(&fetcher.dbserver_failures, 2);
        assert!(fetcher.is_failing(&fetcher.dbserver_failures, 2));
    }

    #[test]
    fn a_failure_older_than_the_retry_window_is_no_longer_skipped() {
        let fetcher = fetcher();
        fetcher.lock(&fetcher.dbserver_failures).insert(2, Instant::now() - RETRY_FAILED_SOURCE_AFTER - Duration::from_secs(1));
        assert!(!fetcher.is_failing(&fetcher.dbserver_failures, 2));
    }

    /// The other half of the same regression: a failure recorded for one
    /// device must never block a *different* device — the whole point of
    /// keying the failure cache by device number rather than sharing one
    /// global flag.
    #[test]
    fn a_failed_device_does_not_affect_a_different_device() {
        let fetcher = fetcher();
        fetcher.mark_failed(&fetcher.dbserver_failures, 2);
        assert!(!fetcher.is_failing(&fetcher.dbserver_failures, 3));
    }

    /// The 2026-08-21 booth capture's shape, at the cache level: deck 3
    /// played off player 4's USB, whose NFS export answered `NOENT`, while
    /// player 2's stick served NFS all night. Keying the cache by the media
    /// -holding player rather than the deck is what keeps player 4's
    /// failure from also blacking out the next track the same deck loads
    /// off player 2.
    #[test]
    fn an_nfs_failure_against_one_media_player_leaves_another_players_media_alone() {
        let fetcher = fetcher();
        fetcher.mark_failed(&fetcher.nfs_failures, 4);
        assert!(fetcher.is_failing(&fetcher.nfs_failures, 4));
        assert!(!fetcher.is_failing(&fetcher.nfs_failures, 2));
    }

    /// The reason there are two separate maps at all: a device failing on
    /// one route must not be treated as failing on the other.
    #[test]
    fn a_failure_on_one_route_does_not_affect_the_other_route() {
        let fetcher = fetcher();
        fetcher.mark_failed(&fetcher.nfs_failures, 2);
        assert!(fetcher.is_failing(&fetcher.nfs_failures, 2));
        assert!(!fetcher.is_failing(&fetcher.dbserver_failures, 2));
    }

    #[test]
    fn forgetting_a_device_clears_both_routes_failure_records() {
        let fetcher = fetcher();
        fetcher.mark_failed(&fetcher.nfs_failures, 2);
        fetcher.mark_failed(&fetcher.dbserver_failures, 2);
        fetcher.forget_device(2);
        assert!(!fetcher.is_failing(&fetcher.nfs_failures, 2));
        assert!(!fetcher.is_failing(&fetcher.dbserver_failures, 2));
    }

    #[test]
    fn a_result_for_the_still_current_track_is_not_superseded() {
        let fetcher = fetcher();
        fetcher.note_current_track(1, Some(4207));
        assert!(!fetcher.is_superseded(1, 4207));
    }

    #[test]
    fn a_result_for_a_track_the_deck_has_since_left_is_superseded() {
        let fetcher = fetcher();
        fetcher.note_current_track(1, Some(4207));
        fetcher.note_current_track(1, Some(9999)); // The deck moved on before the fetch finished.
        assert!(fetcher.is_superseded(1, 4207));
    }
}
