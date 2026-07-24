/**
 * Normalized domain model for Decktective.
 *
 * Everything the UI consumes is expressed with these types — the renderer
 * never sees prolink-connect structures. Both the real Pro DJ Link engine
 * and the simulation engine emit this model.
 */

/** Player number on the Pro DJ Link network (1-4). */
export type DeckId = number

export type PlayState = 'empty' | 'loading' | 'cued' | 'playing' | 'paused' | 'searching' | 'ended'

export type NetworkState = 'offline' | 'searching' | 'connected' | 'failed'

export interface DeviceInfo {
  deckId: DeckId
  name: string
  ip: string
}

/** Live state of a deck, refreshed on every status packet (~5 Hz). */
export interface DeckState {
  deckId: DeckId
  playState: PlayState
  isMaster: boolean
  isSynced: boolean
  isOnAir: boolean
  /** Pitch fader as a percentage, e.g. +2.35 for +2.35% */
  pitch: number
  /** BPM of the loaded track at 0% pitch, null when unknown */
  trackBpm: number | null
  /** trackBpm adjusted by pitch, null when unknown */
  effectiveBpm: number | null
  /** Beat within the current bar (1-4), null when not playing */
  beatInMeasure: number | null
  /** Identifier of the loaded track, null when no track is loaded */
  trackId: number | null
  /**
   * Estimated playhead position in ms, derived from the last beat and the
   * beat grid (Pro DJ Link has no absolute position on this hardware).
   * Null when it cannot be inferred yet.
   */
  playheadMs: number | null
  /** Epoch ms at which this state was computed (interpolation anchor). */
  reportedAt: number
}

export interface BeatGridBeat {
  /** Offset of the beat from the start of the track, in ms */
  offsetMs: number
  /** Position of the beat within its bar (1-4) */
  beatInMeasure: number
}

/**
 * One column of a waveform. `height` is normalized to 0-1,
 * `color` is an [r, g, b] triplet (0-255).
 */
export interface WaveformSegment {
  height: number
  color: [number, number, number]
}

export interface WaveformData {
  /** Detailed waveform (~150 columns per second), null when unavailable */
  detail: WaveformSegment[] | null
  /** How many ms one detail column covers */
  msPerDetailSegment: number
  /** Whole-track preview for the minimap (fixed column count) */
  preview: WaveformSegment[] | null
}

/** Static description of a loaded track, fetched once per track change. */
export interface TrackInfo {
  trackId: number
  title: string
  artist: string | null
  album: string | null
  genre: string | null
  /** Musical key as displayed by Rekordbox, e.g. "8A" or "Am" */
  key: string | null
  label: string | null
  comment: string | null
  durationMs: number
  bpm: number | null
  /** Raw image bytes (JPEG/PNG) of the album artwork */
  artwork: Uint8Array | null
  beatGrid: BeatGridBeat[] | null
  waveform: WaveformData | null
}

/** Full engine state, used to hydrate the UI on (re)connection. */
export interface EngineSnapshot {
  networkState: NetworkState
  devices: DeviceInfo[]
  decks: DeckState[]
  tracks: { deckId: DeckId; track: TrackInfo | null }[]
}
