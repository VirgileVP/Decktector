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
//! Timeouts and the retry backoff are ported from decktector's pre-Rust
//! engine's `track-loader.ts`, whose values came from real four-CDJ-3000
//! testing: `METADATA_TIMEOUT`, `ASSET_TIMEOUT`, `CONNECT_TIMEOUT`,
//! `RETRY_FAILED_SOURCE_AFTER`. `NFS_CONNECT_TIMEOUT`/`NFS_FILE_TIMEOUT` are
//! this phase's own judgment (crate-digger's reference implementation only
//! tunes the *inner* per-RPC-call retransmit loop, not an outer ceiling on
//! a whole multi-call sequence) — reasonable starting points, not
//! hardware-validated.

use std::collections::{HashMap, HashSet};
use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, RwLock};
use std::time::{Duration, Instant};

use decktector_core::model::{BeatGridBeat, DeckId, TrackCue, TrackInfo, WaveformData};
use decktector_core::structure::{self, StructureInput};
use decktector_core::EngineHandle;
use prolink::dbserver::client::TrackMenuItem;
use prolink::dbserver::{beatgrid, cues as db_cues, waveform as db_waveform, Connection};
use prolink::presence::Presence;
use prolink::status::{MediaSlot, TRACK_TYPE_REKORDBOX};
use tokio::sync::Mutex as AsyncMutex;
use tokio::task::JoinHandle;
use tokio::time::timeout;

use super::analysis_wait::{
    player_may_still_be_analysing, AnalysisWait, ANALYSIS_RETRY_INTERVAL, MAXIMUM_ANALYSIS_WAIT,
};
use super::cues::track_cues;
use super::nfs::{self, TrackDatabase};
use super::normalize;

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
const RETRY_FAILED_SOURCE_AFTER: Duration = Duration::from_secs(3);
const PDB_CACHE_TTL: Duration = Duration::from_secs(120);

/// Pooled dbserver connections, keyed by the player holding the media and
/// its address - see [`MetadataFetcher::connections`] for why both halves
/// of the key matter.
type ConnectionPool = HashMap<(u8, Ipv4Addr), Arc<AsyncMutex<Connection>>>;
type PdbCache = HashMap<(Ipv4Addr, MediaSlot), (Arc<TrackDatabase>, Instant)>;

/// Which track a deck has loaded. A rekordbox id is only unique within one
/// player's media slot — every stick numbers its tracks from 1 — so the id
/// alone cannot tell the next DJ's track 15 from the previous DJ's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadedTrack {
    /// The player whose media holds the track — the deck itself on a
    /// stand-alone player, a *different* player on a linked booth (see
    /// `prolink::status::DeviceStatus::track_source_device`). Both the
    /// address and the device number every query names have to be this
    /// player's, not the deck's.
    pub source_device: u8,
    pub slot: MediaSlot,
    pub track_type: u8,
    pub track_id: u32,
}

/// What one status packet supplies about the track it names — everything
/// [`MetadataFetcher::spawn_fetch`] needs beyond what it already tracks
/// itself (the pooled dbserver connection, the failure caches).
pub struct FetchRequest {
    pub deck_id: DeckId,
    pub track: LoadedTrack,
    /// Address of [`LoadedTrack::source_device`].
    pub player_ip: Ipv4Addr,
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
    bind_ip: RwLock<Ipv4Addr>,
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
    connections: Mutex<ConnectionPool>,
    /// Per-route failure records, keyed by the media's player for the same
    /// reason as [`Self::connections`]: a stick with no NFS export must not
    /// stop the deck reading its *next* track off a stick that has one,
    /// and a deck is not what either route talks to.
    nfs_failures: Mutex<HashMap<u8, Instant>>,
    dbserver_failures: Mutex<HashMap<u8, Instant>>,
    /// The track `crate::engine::run_reporting` most recently saw for
    /// each deck — updated by [`Self::note_current_track`] from that same
    /// single-threaded task, read by a spawned fetch task after its network
    /// work completes to decide whether its result is still relevant.
    /// Necessarily duplicates `run_reporting`'s own local map: that map
    /// can't be touched from a spawned task without adding a lock to
    /// `run_reporting`'s hot path, which its own design deliberately
    /// avoids.
    current_track: Mutex<HashMap<DeckId, Option<LoadedTrack>>>,
    active_sources: Mutex<HashMap<DeckId, ActiveMediaSource>>,
    in_flight: Mutex<HashSet<DeckId>>,
    pdb_cache: Mutex<PdbCache>,
    tasks: Mutex<Vec<JoinHandle<()>>>,
}

/// The track whose metadata a deck currently shows, and where it was read.
#[derive(Debug, Clone)]
pub struct ActiveMediaSource {
    pub player_ip: Ipv4Addr,
    pub track: LoadedTrack,
}

impl MetadataFetcher {
    pub fn new(presence: Arc<Presence>, bind_ip: Ipv4Addr) -> Arc<Self> {
        Arc::new(Self {
            presence,
            bind_ip: RwLock::new(bind_ip),
            connections: Mutex::new(HashMap::new()),
            nfs_failures: Mutex::new(HashMap::new()),
            dbserver_failures: Mutex::new(HashMap::new()),
            current_track: Mutex::new(HashMap::new()),
            active_sources: Mutex::new(HashMap::new()),
            in_flight: Mutex::new(HashSet::new()),
            pdb_cache: Mutex::new(HashMap::new()),
            tasks: Mutex::new(Vec::new()),
        })
    }

    /// Updates the local source IP for metadata fetches (e.g. when the
    /// network interface is resolved or changes). Clears any cached
    /// connections and failures so the new interface starts with a clean slate.
    pub fn set_bind_ip(&self, ip: Ipv4Addr) {
        let mut current = self.bind_ip.write().unwrap_or_else(|p| p.into_inner());
        if *current != ip {
            *current = ip;
            self.lock(&self.connections).clear();
            self.lock(&self.nfs_failures).clear();
            self.lock(&self.dbserver_failures).clear();
            self.lock(&self.active_sources).clear();
            self.lock(&self.pdb_cache).clear();
        }
    }

    pub fn bind_ip(&self) -> Ipv4Addr {
        *self.bind_ip.read().unwrap_or_else(|p| p.into_inner())
    }

    pub fn note_current_track(&self, deck_id: DeckId, track: Option<LoadedTrack>) {
        self.lock(&self.current_track).insert(deck_id, track);
        if track.is_none() {
            self.lock(&self.active_sources).remove(&deck_id);
        }
    }

    /// Whether `deck_id` already shows the metadata of exactly `track` —
    /// not merely of a track with the same id off other media.
    pub fn is_showing(&self, deck_id: DeckId, track: &LoadedTrack) -> bool {
        self.lock(&self.active_sources)
            .get(&deck_id)
            .is_some_and(|source| source.track == *track)
    }

    /// Whether a background metadata fetch is currently in progress for `deck_id`.
    pub fn is_fetching(&self, deck_id: DeckId) -> bool {
        self.lock(&self.in_flight).contains(&deck_id)
    }

    /// The database last read for `slot`, unless it has expired. Still to be
    /// checked against the media before use — see [`nfs::load_database`].
    fn cached_database(&self, player_ip: Ipv4Addr, slot: MediaSlot) -> Option<Arc<TrackDatabase>> {
        let mut cache = self.lock(&self.pdb_cache);
        match cache.get(&(player_ip, slot)) {
            Some((database, fetched_at)) if fetched_at.elapsed() < PDB_CACHE_TTL => {
                Some(Arc::clone(database))
            }
            Some(_) => {
                cache.remove(&(player_ip, slot));
                None
            }
            None => None,
        }
    }

    fn update_cached_database(
        &self,
        player_ip: Ipv4Addr,
        slot: MediaSlot,
        database: Arc<TrackDatabase>,
    ) {
        self.lock(&self.pdb_cache)
            .insert((player_ip, slot), (database, Instant::now()));
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
        self.lock(&self.connections)
            .retain(|(pooled_device, _), _| *pooled_device != device);
        self.lock(&self.nfs_failures).remove(&device);
        self.lock(&self.dbserver_failures).remove(&device);
        self.lock(&self.current_track).remove(&device);
        self.lock(&self.active_sources).remove(&device);
        self.lock(&self.in_flight).remove(&device);
    }

    /// Spawns the fetch in the background — never blocks the caller
    /// (`crate::engine::run_reporting`'s status-handling path).
    pub fn spawn_fetch(self: &Arc<Self>, handle: Arc<EngineHandle>, request: FetchRequest) {
        self.lock(&self.in_flight).insert(request.deck_id);
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

    fn is_superseded(&self, deck_id: DeckId, track: &LoadedTrack) -> bool {
        self.lock(&self.current_track).get(&deck_id) != Some(&Some(*track))
    }

    async fn fetch_and_report(self: Arc<Self>, handle: Arc<EngineHandle>, request: FetchRequest) {
        let FetchRequest {
            deck_id,
            track,
            player_ip,
            status_bpm,
        } = request;
        let LoadedTrack {
            source_device,
            slot,
            track_type,
            track_id,
        } = track;

        struct InFlightGuard {
            fetcher: Arc<MetadataFetcher>,
            deck_id: DeckId,
        }
        impl Drop for InFlightGuard {
            fn drop(&mut self) {
                self.fetcher
                    .lock(&self.fetcher.in_flight)
                    .remove(&self.deck_id);
            }
        }
        let in_flight = InFlightGuard {
            fetcher: Arc::clone(&self),
            deck_id,
        };

        // Neither route takes `deck_id`: everything they key on — the
        // connection pool, both failure caches, every query field — belongs
        // to the player holding the media. Keeping the deck out of scope
        // there is what stops the two `u8`s being swapped again.
        let from_database = if nfs_route_applies(track_type) {
            self.try_nfs(source_device, player_ip, slot, track_id, status_bpm)
                .await
        } else {
            None
        };
        let (track_info, awaiting_analysis) = match from_database {
            Some(track_info) => (track_info, None),
            None => {
                let Some(dbserver_track) = self.try_dbserver(player_ip, &track).await else {
                    return; // Both routes failed or had nothing for this track — leave the deck's last-known metadata as is.
                };
                let track_info = dbserver_track.to_track_info(track_id, status_bpm);
                let awaiting_analysis = (player_may_still_be_analysing(track_type)
                    && !dbserver_track.analysis.is_complete())
                .then_some(dbserver_track);
                (track_info, awaiting_analysis)
            }
        };

        if self.is_superseded(deck_id, &track) {
            return;
        }
        self.lock(&self.active_sources)
            .insert(deck_id, ActiveMediaSource { player_ip, track });
        handle.update_deck_track(deck_id, Some(track_info));

        // The deck shows its metadata now: no fetch is in flight for it any
        // more, even while the player's own analysis is still awaited.
        drop(in_flight);
        if let Some(dbserver_track) = awaiting_analysis {
            self.await_player_analysis(
                &handle,
                deck_id,
                track,
                player_ip,
                status_bpm,
                dbserver_track,
            )
            .await;
        }
    }

    /// Asks again, on [`AnalysisWait`]'s schedule, for the beat grid and
    /// waveform the player is still computing for a track it loaded without
    /// rekordbox analysis, and shows each new piece as it arrives. Stops
    /// once both are in, the deck has moved on, or the wait runs out.
    async fn await_player_analysis(
        &self,
        handle: &EngineHandle,
        deck_id: DeckId,
        track: LoadedTrack,
        player_ip: Ipv4Addr,
        status_bpm: Option<f64>,
        mut dbserver_track: DbserverTrack,
    ) {
        tracing::debug!(
            deck_id,
            track_id = track.track_id,
            "waiting for the player to analyse the track"
        );
        let wait = AnalysisWait::new(ANALYSIS_RETRY_INTERVAL, MAXIMUM_ANALYSIS_WAIT);
        while wait.next_attempt().await {
            if self.is_superseded(deck_id, &track) {
                return;
            }
            let fresh = self.fetch_analysis_again(player_ip, &track).await;
            if !dbserver_track.analysis.absorb(fresh) {
                continue;
            }
            if self.is_superseded(deck_id, &track) {
                return;
            }
            handle.update_deck_track(
                deck_id,
                Some(dbserver_track.to_track_info(track.track_id, status_bpm)),
            );
            if dbserver_track.analysis.is_complete() {
                return;
            }
        }
        tracing::debug!(
            deck_id,
            track_id = track.track_id,
            "gave up waiting for the player's analysis"
        );
    }

    /// One more request for `track`'s beat grid and waveform, on the pooled
    /// connection to the player holding it. Nothing on any failure: the
    /// next attempt simply asks again.
    async fn fetch_analysis_again(
        &self,
        player_ip: Ipv4Addr,
        track: &LoadedTrack,
    ) -> DbserverAnalysis {
        if self.is_failing(&self.dbserver_failures, track.source_device) {
            return DbserverAnalysis::default();
        }
        let Some(connection) = self.connection_for(track.source_device, player_ip).await else {
            return DbserverAnalysis::default();
        };
        let mut connection = connection.lock().await;
        if self.presence.device_number().is_none() {
            return DbserverAnalysis::default(); // Same gate as `try_dbserver`.
        }
        fetch_dbserver_analysis(&mut connection, track).await
    }

    /// Downloads the audio file currently loaded on `deck_id` over NFS into `destination_dir`.
    pub async fn download_track(
        &self,
        deck_id: DeckId,
        destination_dir: std::path::PathBuf,
        handle: &EngineHandle,
        progress_callback: Box<dyn Fn(u64, u64) + Send + Sync>,
    ) -> anyhow::Result<std::path::PathBuf> {
        let active_source = self
            .lock(&self.active_sources)
            .get(&deck_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("no active media source known for deck {deck_id}"))?;

        let ActiveMediaSource {
            player_ip,
            track: LoadedTrack { slot, track_id, .. },
        } = active_source;
        let mut client = prolink::nfs::NfsClient::open(player_ip, self.bind_ip(), slot).await?;
        let cached = self.cached_database(player_ip, slot);
        let (database, is_new) = nfs::load_database(&mut client, cached).await?;
        if is_new {
            self.update_cached_database(player_ip, slot, Arc::clone(&database));
        }

        let track = database
            .pdb
            .tracks
            .get(&track_id)
            .ok_or_else(|| anyhow::anyhow!("track {track_id} not found in database"))?;

        let raw_file_path = track.file_path.as_deref().ok_or_else(|| {
            anyhow::anyhow!("no audio file path recorded in database for this track")
        })?;

        let extension = std::path::Path::new(raw_file_path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("mp3");

        let current_track_info = handle
            .snapshot()
            .tracks
            .into_iter()
            .find(|t| t.deck_id == deck_id)
            .and_then(|t| t.track);

        let artist = current_track_info
            .as_ref()
            .and_then(|t| t.artist.as_deref())
            .unwrap_or("Unknown Artist");
        let title = current_track_info
            .as_ref()
            .map(|t| t.title.as_str())
            .unwrap_or(&track.title);

        let clean_artist = artist.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");
        let clean_title = title.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");
        let filename = format!("{clean_artist} - {clean_title}.{extension}");
        let dest_file = destination_dir.join(&filename);

        tokio::fs::create_dir_all(&destination_dir).await?;

        client
            .download_file_to_path(raw_file_path, &dest_file, move |downloaded, total| {
                progress_callback(downloaded, total);
            })
            .await?;

        Ok(dest_file)
    }

    /// The NFS route — see [`super::nfs`]. Uses in-memory caching of `export.pdb` /
    /// `exportLibrary.db` across requests to avoid hammering linked media sources.
    async fn try_nfs(
        &self,
        source_device: u8,
        player_ip: Ipv4Addr,
        slot: MediaSlot,
        track_id: u32,
        status_bpm: Option<f64>,
    ) -> Option<TrackInfo> {
        if self.is_failing(&self.nfs_failures, source_device) {
            return None;
        }
        let cached_database = self.cached_database(player_ip, slot);
        let fetch = nfs::fetch_track_info(
            player_ip,
            self.bind_ip(),
            slot,
            track_id,
            status_bpm,
            cached_database,
        );
        match timeout(NFS_CONNECT_TIMEOUT + NFS_FILE_TIMEOUT, fetch).await {
            Ok(Ok((track_info, new_database))) => {
                if let Some(database) = new_database {
                    self.update_cached_database(player_ip, slot, database);
                }
                track_info
            }
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
        player_ip: Ipv4Addr,
        track: &LoadedTrack,
    ) -> Option<DbserverTrack> {
        let LoadedTrack {
            source_device,
            slot,
            track_type,
            track_id,
        } = *track;
        if self.is_failing(&self.dbserver_failures, source_device) {
            return None;
        }
        let connection = self.connection_for(source_device, player_ip).await?;

        let mut connection = connection.lock().await;
        // Only a gate: our own number identified us once, in the setup
        // message `Connection::open` sent. Losing it mid-fetch means the
        // connection no longer speaks for anyone. Every *query* below names
        // the player being asked about instead — passing our own number
        // there gets an empty menu back from real hardware.
        self.presence.device_number()?;

        let items = match timeout(
            METADATA_TIMEOUT,
            connection.fetch_metadata(source_device, slot.to_byte(), track_type, track_id),
        )
        .await
        {
            Ok(Ok(items)) if !items.is_empty() => items,
            Ok(Ok(_)) => return None, // The player answered: this track has no metadata.
            _ => {
                self.mark_failed(&self.dbserver_failures, source_device);
                self.lock(&self.connections)
                    .remove(&(source_device, player_ip));
                return None;
            }
        };

        let analysis = fetch_dbserver_analysis(&mut connection, track).await;
        Some(DbserverTrack { items, analysis })
    }

    /// The pooled dbserver connection to the player holding the media,
    /// opening a fresh one (bounded by [`CONNECT_TIMEOUT`]) if there isn't
    /// one yet. Keyed by device *and* address — see [`Self::connections`]
    /// for why the deck is not part of this.
    async fn connection_for(
        &self,
        source_device: u8,
        player_ip: Ipv4Addr,
    ) -> Option<Arc<AsyncMutex<Connection>>> {
        let key = (source_device, player_ip);
        if let Some(existing) = self.lock(&self.connections).get(&key).cloned() {
            return Some(existing);
        }

        let our_number = self.presence.device_number()?;
        let opened = timeout(
            CONNECT_TIMEOUT,
            Connection::open(player_ip, self.bind_ip(), our_number),
        )
        .await;
        match opened {
            Ok(Ok(connection)) => {
                let wrapped = Arc::new(AsyncMutex::new(connection));
                self.lock(&self.connections)
                    .insert(key, Arc::clone(&wrapped));
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

/// Whether the NFS route can answer for a track of this kind. It looks the
/// id up in the media's track database, which only lists rekordbox-analysed
/// tracks: any other id would hit an unrelated row and show the wrong track.
fn nfs_route_applies(track_type: u8) -> bool {
    track_type == TRACK_TYPE_REKORDBOX
}

/// What the dbserver route read for a track: its menu items, and whatever
/// analysis the player could give at the time.
struct DbserverTrack {
    items: Vec<TrackMenuItem>,
    analysis: DbserverAnalysis,
}

impl DbserverTrack {
    fn to_track_info(&self, track_id: u32, status_bpm: Option<f64>) -> TrackInfo {
        assemble_dbserver_track_info(track_id, &self.items, status_bpm, &self.analysis)
    }
}

/// A track's beat grid, waveform and cues as read from the dbserver. The
/// beat grid and waveform are each `None` until the player has one to give
/// — an empty answer counts as none, since that is what a player still
/// analysing a track sends. No cues is an ordinary answer, so it does not
/// keep the analysis incomplete.
#[derive(Debug, Default)]
struct DbserverAnalysis {
    beat_grid: Option<Vec<BeatGridBeat>>,
    waveform: Option<WaveformData>,
    cues: Vec<TrackCue>,
}

impl DbserverAnalysis {
    fn from_blobs(beat_grid_blob: Option<&[u8]>, waveform_blob: Option<&[u8]>) -> Self {
        Self {
            beat_grid: beat_grid_blob
                .map(|blob| normalize::beat_grid(&beatgrid::parse_beat_grid(blob)))
                .filter(|grid| !grid.is_empty()),
            waveform: waveform_blob.and_then(|blob| {
                normalize::waveform_data(&db_waveform::parse_color_waveform_detail(blob))
            }),
            cues: Vec::new(),
        }
    }

    /// The same analysis, with the cues a basic cue list reply describes.
    fn with_cue_points(self, cue_point_blobs: &[Vec<u8>]) -> Self {
        Self {
            cues: track_cues(&db_cues::parse_cue_points(cue_point_blobs)),
            ..self
        }
    }

    fn is_complete(&self) -> bool {
        self.beat_grid.is_some() && self.waveform.is_some()
    }

    /// Takes from `fresh` whatever this is still missing; whether it took
    /// anything. What is already held is kept, never replaced.
    fn absorb(&mut self, fresh: Self) -> bool {
        let mut took_any = false;
        if self.beat_grid.is_none() && fresh.beat_grid.is_some() {
            self.beat_grid = fresh.beat_grid;
            took_any = true;
        }
        if self.waveform.is_none() && fresh.waveform.is_some() {
            self.waveform = fresh.waveform;
            took_any = true;
        }
        if self.cues.is_empty() && !fresh.cues.is_empty() {
            self.cues = fresh.cues;
            took_any = true;
        }
        took_any
    }
}

/// Requests `track`'s beat grid, waveform and cues on an open dbserver
/// connection, each bounded by [`ASSET_TIMEOUT`]. A failed or timed-out
/// request reads as "not available", like an empty answer.
async fn fetch_dbserver_analysis(
    connection: &mut Connection,
    track: &LoadedTrack,
) -> DbserverAnalysis {
    let LoadedTrack {
        source_device,
        slot,
        track_type,
        track_id,
    } = *track;
    let slot = slot.to_byte();
    let beat_grid_blob = timeout(
        ASSET_TIMEOUT,
        connection.fetch_beat_grid(source_device, slot, track_type, track_id),
    )
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
    let cue_point_blobs = timeout(
        ASSET_TIMEOUT,
        connection.fetch_cues_basic(source_device, slot, track_type, track_id),
    )
    .await
    .ok()
    .and_then(Result::ok)
    .unwrap_or_default();
    DbserverAnalysis::from_blobs(beat_grid_blob.as_deref(), waveform_blob.as_deref())
        .with_cue_points(&cue_point_blobs)
}

fn assemble_dbserver_track_info(
    track_id: u32,
    items: &[TrackMenuItem],
    status_bpm: Option<f64>,
    analysis: &DbserverAnalysis,
) -> TrackInfo {
    let beat_grid = analysis.beat_grid.clone();
    let waveform = analysis.waveform.clone();
    let duration_ms = normalize::duration_ms(items);
    let cues = analysis.cues.clone();

    // Borrowed here, before the same values move into the TrackInfo literal
    // below — mirrors decktector_core::sim::tracks::build_sim_track's
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
    use prolink::dbserver::queries::MenuItemType;

    fn menu_item(item_type: MenuItemType, text: &str) -> TrackMenuItem {
        TrackMenuItem {
            item_type,
            text: Some(text.to_string()),
            number: None,
        }
    }

    #[test]
    fn assembles_a_track_info_from_menu_items_alone() {
        let items = vec![
            menu_item(MenuItemType::TrackTitle, "Strobe"),
            menu_item(MenuItemType::Artist, "deadmau5"),
        ];
        let track =
            assemble_dbserver_track_info(4207, &items, Some(128.0), &DbserverAnalysis::default());

        assert_eq!(track.track_id, 4207);
        assert_eq!(track.title, "Strobe");
        assert_eq!(track.artist.as_deref(), Some("deadmau5"));
        assert_eq!(track.bpm, Some(128.0));
        assert!(track.beat_grid.is_none());
        assert!(track.waveform.is_none());
        assert!(
            track.events.is_empty(),
            "no beat grid/waveform means nothing to analyse"
        );
    }

    #[test]
    fn a_missing_title_item_falls_back_to_an_empty_title_rather_than_panicking() {
        let track = assemble_dbserver_track_info(1, &[], None, &DbserverAnalysis::default());
        assert_eq!(track.title, "");
    }

    /// One full-height white column behind the leading bytes the dbserver
    /// waveform parser skips.
    fn waveform_blob() -> Vec<u8> {
        let mut blob = vec![0u8; 28];
        let bits: u16 = (31u16 << 2) | (7u16 << 13) | (7u16 << 10) | (7u16 << 7);
        blob.extend_from_slice(&bits.to_be_bytes());
        blob
    }

    /// One beat at 128 BPM behind the header the beat grid parser skips.
    fn beat_grid_blob() -> Vec<u8> {
        let mut blob = vec![0u8; 20];
        blob.extend_from_slice(&1u16.to_le_bytes());
        blob.extend_from_slice(&12_800u16.to_le_bytes());
        blob.extend_from_slice(&0u32.to_le_bytes());
        blob.extend_from_slice(&[0u8; 8]);
        blob
    }

    /// What a player still analysing an unanalysed file answers: blobs too
    /// short to hold anything. That must read as "not there yet", or the
    /// fetcher would stop waiting for the real analysis.
    #[test]
    fn empty_analysis_answers_count_as_missing() {
        let analysis = DbserverAnalysis::from_blobs(Some(&[0u8; 4]), Some(&[0u8; 4]));
        assert!(analysis.beat_grid.is_none());
        assert!(analysis.waveform.is_none());
        assert!(!analysis.is_complete());
    }

    #[test]
    fn analysis_with_both_beat_grid_and_waveform_is_complete() {
        let analysis =
            DbserverAnalysis::from_blobs(Some(&beat_grid_blob()), Some(&waveform_blob()));
        assert!(analysis.is_complete());
    }

    #[test]
    fn absorbing_takes_only_what_is_still_missing() {
        let mut held = DbserverAnalysis::from_blobs(Some(&beat_grid_blob()), None);
        let fresh = DbserverAnalysis::from_blobs(None, Some(&waveform_blob()));

        assert!(held.absorb(fresh));
        assert!(held.is_complete());
    }

    /// A later attempt that gets less than an earlier one must not wipe
    /// what the deck already shows.
    #[test]
    fn absorbing_an_emptier_answer_keeps_what_is_held_and_reports_nothing_new() {
        let mut held = DbserverAnalysis::from_blobs(Some(&beat_grid_blob()), None);

        assert!(!held.absorb(DbserverAnalysis::default()));
        assert!(held.beat_grid.is_some());
    }

    /// Hot cue B at 63.9 s, as a basic cue list reply carries it: one
    /// 36-byte record, then its time and "no loop".
    fn one_hot_cue_blobs() -> Vec<Vec<u8>> {
        let mut record = vec![0u8; db_cues::CUE_RECORD_LEN];
        record[1] = 0x01;
        record[2] = 2;
        let times = [63_902u32.to_le_bytes(), u32::MAX.to_le_bytes()].concat();
        vec![record, times]
    }

    /// The dbserver fallback used to hand every track over with no cues at
    /// all, whatever the player held.
    #[test]
    fn a_dbserver_track_carries_the_cues_its_player_listed() {
        let analysis = DbserverAnalysis::default().with_cue_points(&one_hot_cue_blobs());

        let track = assemble_dbserver_track_info(1, &[], None, &analysis);

        assert_eq!(track.cues.len(), 1);
        assert_eq!(track.cues[0].button.as_deref(), Some("B"));
        assert_eq!(track.cues[0].offset_ms, 63_902.0);
    }

    #[test]
    fn a_track_without_cues_is_still_complete() {
        let analysis =
            DbserverAnalysis::from_blobs(Some(&beat_grid_blob()), Some(&waveform_blob()))
                .with_cue_points(&[Vec::new(), Vec::new()]);
        assert!(analysis.is_complete());
        assert!(analysis.cues.is_empty());
    }

    #[test]
    fn absorbing_takes_cues_a_later_answer_found() {
        let mut held = DbserverAnalysis::from_blobs(Some(&beat_grid_blob()), None);
        let fresh = DbserverAnalysis::default().with_cue_points(&one_hot_cue_blobs());

        assert!(held.absorb(fresh));
        assert_eq!(held.cues.len(), 1);
    }

    fn fetcher() -> Arc<MetadataFetcher> {
        MetadataFetcher::new(Presence::new(None), Ipv4Addr::LOCALHOST)
    }

    /// The `b5cbf4f`-class regression, at the orchestration level: a device
    /// that just failed to connect is skipped (not re-attempted) on the
    /// very next track change, so one bad player doesn't spend a fresh
    /// timeout on every subsequent track it loads.
    #[test]
    fn the_nfs_route_answers_for_rekordbox_tracks() {
        assert!(nfs_route_applies(TRACK_TYPE_REKORDBOX));
    }

    /// An unanalysed file loaded from the browser, or a CD track, carries an
    /// id that is not a database row: looking it up there showed whichever
    /// rekordbox track happened to share the number.
    #[test]
    fn the_nfs_route_leaves_other_track_kinds_to_dbserver() {
        const UNANALYSED_FILE: u8 = 0x02;
        const AUDIO_CD: u8 = 0x05;
        assert!(!nfs_route_applies(UNANALYSED_FILE));
        assert!(!nfs_route_applies(AUDIO_CD));
    }

    #[test]
    fn a_freshly_failed_device_is_skipped() {
        let fetcher = fetcher();
        fetcher.mark_failed(&fetcher.dbserver_failures, 2);
        assert!(fetcher.is_failing(&fetcher.dbserver_failures, 2));
    }

    #[test]
    fn a_failure_older_than_the_retry_window_is_no_longer_skipped() {
        let fetcher = fetcher();
        fetcher.lock(&fetcher.dbserver_failures).insert(
            2,
            Instant::now() - RETRY_FAILED_SOURCE_AFTER - Duration::from_secs(1),
        );
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

    fn loaded(source_device: u8, track_id: u32) -> LoadedTrack {
        LoadedTrack {
            source_device,
            slot: MediaSlot::Usb,
            track_type: TRACK_TYPE_REKORDBOX,
            track_id,
        }
    }

    #[test]
    fn a_result_for_the_still_current_track_is_not_superseded() {
        let fetcher = fetcher();
        fetcher.note_current_track(1, Some(loaded(1, 4207)));
        assert!(!fetcher.is_superseded(1, &loaded(1, 4207)));
    }

    #[test]
    fn a_result_for_a_track_the_deck_has_since_left_is_superseded() {
        let fetcher = fetcher();
        fetcher.note_current_track(1, Some(loaded(1, 4207)));
        fetcher.note_current_track(1, Some(loaded(1, 9999))); // The deck moved on before the fetch finished.
        assert!(fetcher.is_superseded(1, &loaded(1, 4207)));
    }

    /// Same id, other media: the deck loaded track 15 off its own stick
    /// after track 15 off the linked player's. The first result must not
    /// count as the second's.
    #[test]
    fn the_same_id_off_other_media_is_another_track() {
        let fetcher = fetcher();
        let linked_players_track = loaded(2, 15);
        let own_track = loaded(1, 15);
        fetcher.note_current_track(1, Some(own_track));
        fetcher.lock(&fetcher.active_sources).insert(
            1,
            ActiveMediaSource {
                player_ip: Ipv4Addr::LOCALHOST,
                track: linked_players_track,
            },
        );

        assert!(fetcher.is_superseded(1, &linked_players_track));
        assert!(!fetcher.is_showing(1, &own_track));
        assert!(fetcher.is_showing(1, &linked_players_track));
    }

    #[test]
    fn updating_bind_ip_changes_ip_and_clears_failure_records() {
        let fetcher = fetcher();
        assert_eq!(fetcher.bind_ip(), Ipv4Addr::LOCALHOST);

        fetcher.mark_failed(&fetcher.nfs_failures, 2);
        fetcher.mark_failed(&fetcher.dbserver_failures, 2);
        assert!(fetcher.is_failing(&fetcher.nfs_failures, 2));

        let new_ip: Ipv4Addr = "192.168.1.50".parse().unwrap();
        fetcher.set_bind_ip(new_ip);
        assert_eq!(fetcher.bind_ip(), new_ip);
        assert!(!fetcher.is_failing(&fetcher.nfs_failures, 2));
        assert!(!fetcher.is_failing(&fetcher.dbserver_failures, 2));
    }

    #[test]
    fn in_flight_and_pdb_cache_work_as_expected() {
        let fetcher = fetcher();
        assert!(!fetcher.is_fetching(1));

        fetcher.lock(&fetcher.in_flight).insert(1);
        assert!(fetcher.is_fetching(1));

        fetcher.forget_device(1);
        assert!(!fetcher.is_fetching(1));

        let ip: Ipv4Addr = "192.168.1.4".parse().unwrap();
        assert!(fetcher.cached_database(ip, MediaSlot::Usb).is_none());

        let database = Arc::new(TrackDatabase::new(
            prolink::nfs::pdb::Pdb::default(),
            "PIONEER/rekordbox/export.pdb",
            prolink::nfs::procedures::FileAttributes::default(),
        ));
        fetcher.update_cached_database(ip, MediaSlot::Usb, database);
        assert!(fetcher.cached_database(ip, MediaSlot::Usb).is_some());

        fetcher.set_bind_ip("192.168.1.100".parse().unwrap());
        assert!(fetcher.cached_database(ip, MediaSlot::Usb).is_none());
    }
}
