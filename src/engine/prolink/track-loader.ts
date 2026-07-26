import type { CDJStatus, ConnectedProlinkNetwork, Track, WaveformHD } from 'prolink-connect'
// The remotedb query catalogue is not re-exported from the package root, but
// its declarations are shipped — types only, no runtime import.
import type { QueryDescriptor } from 'prolink-connect/lib/remotedb'

import type { TrackInfo, WaveformData } from '../types'
import { buildPreviewFromDetail } from '../waveform'
import { mapCues } from './cues'
import { diag, slotName, TimeoutError, trackTypeName, withTimeout } from './diagnostics'

/** HD waveforms carry 150 columns per second of audio. */
const HD_SEGMENTS_PER_SECOND = 150
/** Height of a raw waveform column (0-31). */
const MAX_SEGMENT_HEIGHT = 31

/**
 * Generous: the first lookup for a media device downloads and hydrates its
 * whole `export.pdb`, which takes a while on a large library. Still bounded,
 * because the alternative — prolink-connect's own unbounded waits — shows up
 * as a deck that simply never fills in.
 */
const METADATA_TIMEOUT_MS = 30_000
/** Artwork, waveforms and cues come after the database is already warm. */
const ASSET_TIMEOUT_MS = 15_000
/** Opening a remote database connection is a handshake, not a transfer. */
const CONNECT_TIMEOUT_MS = 10_000
/**
 * How long a media source stays written off after a failed lookup. Long
 * enough not to retry a dead route on every track change, short enough that
 * swapping a USB mid-set recovers on its own.
 */
const RETRY_FAILED_SOURCE_AFTER_MS = 60_000

/**
 * Request ids of the remote database protocol.
 *
 * prolink-connect exposes `remotedb` on the network but not the `Query` enum
 * naming these requests, so the wire values are spelled out here. They match
 * `src/remotedb/message/types.ts` (Deep Symmetry's dbserver documentation).
 */
const REMOTE_QUERY = {
  metadata: 0x2002,
  artwork: 0x2003,
  beatGrid: 0x2204,
  cuesAdv: 0x2b04,
  waveformHd: 0x2c04
} as const

/** `MenuTarget.Main` — the only menu target the players use for lookups. */
const MENU_TARGET_MAIN = 1

/** Everything gathered about a track, whichever route provided it. */
interface TrackAssets {
  track: Track
  artwork: Buffer | Uint8Array | null
  waveformHd: WaveformHD | null
}

function convertWaveform(waveformHd: WaveformHD | null): WaveformData | null {
  if (!waveformHd || waveformHd.length === 0) return null

  const detail = waveformHd.map((segment) => ({
    height: segment.height / MAX_SEGMENT_HEIGHT,
    // prolink-connect color channels are 0-1 floats
    color: [
      Math.round(segment.color[0] * 255),
      Math.round(segment.color[1] * 255),
      Math.round(segment.color[2] * 255)
    ] as [number, number, number]
  }))

  return {
    detail,
    msPerDetailSegment: 1000 / HD_SEGMENTS_PER_SECOND,
    preview: buildPreviewFromDetail(detail)
  }
}

/**
 * Rekordbox databases store tempo as BPM×100 in some code paths; normalize
 * to actual BPM either way.
 */
function normalizeTempo(tempo: number): number | null {
  if (!tempo || tempo <= 0) return null
  return tempo > 400 ? tempo / 100 : tempo
}

/** Shapes whatever the lookup produced into the normalized domain model. */
function toTrackInfo(
  { track, artwork, waveformHd }: TrackAssets,
  statusBpm: number | null
): TrackInfo {
  return {
    trackId: track.id,
    title: track.title,
    artist: track.artist?.name ?? null,
    album: track.album?.name ?? null,
    genre: track.genre?.name ?? null,
    key: track.key?.name ?? null,
    label: track.label?.name ?? null,
    comment: track.comment || null,
    durationMs: track.duration * 1000,
    bpm: normalizeTempo(track.tempo) ?? statusBpm,
    artwork: artwork ? new Uint8Array(artwork) : null,
    beatGrid:
      track.beatGrid?.map((beat) => ({
        offsetMs: beat.offset,
        beatInMeasure: beat.count
      })) ?? null,
    waveform: convertWaveform(waveformHd ?? track.waveformHd),
    cues: mapCues(track.cueAndLoops)
  }
}

/**
 * Fetches everything about the track referenced by a status packet:
 * metadata, artwork, waveforms and cues.
 *
 * Two routes exist, and which one applies is decided by prolink-connect from
 * the track type: Rekordbox-analyzed media in a player is read out of band
 * over NFS (the "local" database — full waveforms and cues), while anything
 * else goes through the players' remote database service.
 *
 * That first route is the only one prolink-connect will consider for analyzed
 * media, and it depends on the player answering a media-slot query and
 * serving its `export.pdb` over NFS — see {@link installMediaSlotBypass} for
 * the first half. When it comes up empty, this falls back to asking the
 * player directly over the remote database protocol, which is enough for
 * title, artist, key, artwork, beat grid and HD waveform.
 *
 * Observed on a four-CDJ-3000 network: the remote route needs the virtual CDJ
 * to announce a player number in the 1-4 range. With all four taken, players
 * accept the connection and then drop the query, so this fallback only pays
 * off once DECKTECTIVE_VCDJ_ID puts us in range.
 */
export class TrackLoader {
  /**
   * When each route last failed, keyed `local:<device>-<slot>` and
   * `remote:<device>`. Avoids re-waiting on a route that is currently dead
   * for every track change, without giving up on it for the whole session.
   */
  private failures = new Map<string, number>()

  /** Forgets the failure history of a device that just left the network. */
  forgetDevice(deviceId: number): void {
    for (const key of this.failures.keys()) {
      if (key.endsWith(`:${deviceId}`) || key.includes(`:${deviceId}-`)) this.failures.delete(key)
    }
  }

  private isFailing(key: string): boolean {
    const failedAt = this.failures.get(key)
    if (failedAt === undefined) return false
    if (Date.now() - failedAt < RETRY_FAILED_SOURCE_AFTER_MS) return true
    this.failures.delete(key)
    return false
  }

  async load(network: ConnectedProlinkNetwork, status: CDJStatus.State): Promise<TrackInfo | null> {
    const context = {
      deck: status.deviceId,
      trackId: status.trackId,
      source: status.trackDeviceId,
      slot: slotName(status.trackSlot),
      type: trackTypeName(status.trackType)
    }
    diag('track:load', context)

    const localKey = `local:${status.trackDeviceId}-${status.trackSlot}`
    if (this.isFailing(localKey)) {
      diag('track:skip-database', { ...context, reason: 'route failed recently' })
    } else {
      const track = await this.loadViaDatabase(network, status, localKey, context)
      if (track) return track
    }

    const remoteKey = `remote:${status.trackDeviceId}`
    if (this.isFailing(remoteKey)) {
      diag('track:skip-remotedb', { ...context, reason: 'route failed recently' })
      return null
    }
    return this.loadViaRemotedb(network, status, remoteKey, context)
  }

  /** The route prolink-connect picks on its own (NFS for analyzed media). */
  private async loadViaDatabase(
    network: ConnectedProlinkNetwork,
    status: CDJStatus.State,
    failureKey: string,
    context: Record<string, unknown>
  ): Promise<TrackInfo | null> {
    const query = {
      deviceId: status.trackDeviceId,
      trackSlot: status.trackSlot as never,
      trackType: status.trackType as never,
      trackId: status.trackId
    }
    const startedAt = Date.now()

    try {
      const track = await withTimeout(
        network.db.getMetadata(query),
        METADATA_TIMEOUT_MS,
        'db.getMetadata'
      )
      if (!track) {
        // No strategy applied, or the media holds no such track: nothing to
        // retry here, the remote route is the only remaining option.
        this.failures.set(failureKey, Date.now())
        diag('track:database-empty', { ...context, ms: Date.now() - startedAt })
        return null
      }

      const [artwork, waveforms] = await Promise.all([
        this.bestEffort('db.getArtwork', context, () => network.db.getArtwork({ ...query, track })),
        this.bestEffort('db.getWaveforms', context, () =>
          network.db.getWaveforms({ ...query, track })
        )
      ])

      diag('track:loaded', {
        ...context,
        via: 'database',
        title: track.title,
        artwork: artwork !== null,
        waveform: (waveforms?.waveformHd ?? track.waveformHd) !== null,
        cues: track.cueAndLoops?.length ?? 0,
        ms: Date.now() - startedAt
      })
      return toTrackInfo(
        { track, artwork, waveformHd: waveforms?.waveformHd ?? null },
        status.trackBPM
      )
    } catch (error) {
      // A timeout means something inside is still waiting — and in the media
      // slot query's case, still holding its lock — so back off this route.
      if (error instanceof TimeoutError) this.failures.set(failureKey, Date.now())
      diag('track:database-failed', { ...context, error, ms: Date.now() - startedAt })
      return null
    }
  }

  /** Asks the player's own database service, the way another CDJ would. */
  private async loadViaRemotedb(
    network: ConnectedProlinkNetwork,
    status: CDJStatus.State,
    failureKey: string,
    context: Record<string, unknown>
  ): Promise<TrackInfo | null> {
    const startedAt = Date.now()
    const trackId = status.trackId

    try {
      const conn = await withTimeout(
        network.remotedb.get(status.trackDeviceId),
        CONNECT_TIMEOUT_MS,
        'remotedb.get'
      )
      if (!conn) {
        this.failures.set(failureKey, Date.now())
        diag('track:no-remotedb', context)
        return null
      }

      const descriptor = {
        menuTarget: MENU_TARGET_MAIN,
        trackSlot: status.trackSlot,
        trackType: status.trackType
      } as QueryDescriptor

      const track = await withTimeout(
        conn.query({
          queryDescriptor: descriptor,
          query: REMOTE_QUERY.metadata,
          args: { trackId }
        }),
        METADATA_TIMEOUT_MS,
        'remotedb metadata'
      )

      // Extras are requested one at a time: a remote database connection is a
      // single socket, serialized behind one lock. Each is best-effort — a
      // missing waveform costs the waveform, not the whole track.
      track.beatGrid = await this.bestEffort('beatGrid', context, () =>
        conn.query({ queryDescriptor: descriptor, query: REMOTE_QUERY.beatGrid, args: { trackId } })
      )
      track.cueAndLoops = await this.bestEffort('cues', context, () =>
        conn.query({ queryDescriptor: descriptor, query: REMOTE_QUERY.cuesAdv, args: { trackId } })
      )
      const waveformHd = await this.bestEffort('waveformHd', context, () =>
        conn.query({
          queryDescriptor: descriptor,
          query: REMOTE_QUERY.waveformHd,
          args: { trackId }
        })
      )
      const artworkId = track.artwork?.id
      const artwork =
        artworkId === undefined
          ? null
          : await this.bestEffort('artwork', context, () =>
              conn.query({
                queryDescriptor: descriptor,
                query: REMOTE_QUERY.artwork,
                args: { artworkId }
              })
            )

      diag('track:loaded', {
        ...context,
        via: 'remotedb',
        title: track.title,
        artwork: artwork !== null,
        waveform: waveformHd !== null,
        cues: track.cueAndLoops?.length ?? 0,
        ms: Date.now() - startedAt
      })
      return toTrackInfo({ track, artwork, waveformHd }, status.trackBPM)
    } catch (error) {
      this.failures.set(failureKey, Date.now())
      diag('track:remotedb-failed', { ...context, error, ms: Date.now() - startedAt })
      return null
    }
  }

  /** Runs one optional lookup; a failure costs that asset, not the track. */
  private async bestEffort<T>(
    label: string,
    context: Record<string, unknown>,
    run: () => Promise<T>
  ): Promise<T | null> {
    try {
      return await withTimeout(run(), ASSET_TIMEOUT_MS, label)
    } catch (error) {
      diag('track:asset-failed', { ...context, asset: label, error })
      return null
    }
  }
}
