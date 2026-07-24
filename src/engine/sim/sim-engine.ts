import { BaseEngine } from '../engine'
import type { DeckId, PlayState, TrackInfo } from '../types'
import { SIM_TRACKS, buildSimTrack } from './sim-tracks'

const TICK_MS = 200
/** Time before the end of a track at which the sim deck loads the next one. */
const TRACK_SWAP_MARGIN_MS = 8000
const TRACK_LOAD_DELAY_MS = 1500
const CUE_TO_PLAY_DELAY_MS = 2500

/** Per-tick probabilities of spontaneous DJ actions on a playing deck. */
const ACTION_ODDS = {
  pause: 0.004,
  cueJump: 0.006,
  scratch: 0.004
} as const
const PAUSE_MIN_MS = 1000
const PAUSE_MAX_MS = 4000
const SCRATCH_MS = 800

interface SimDeck {
  deckId: DeckId
  playState: PlayState
  track: TrackInfo | null
  playheadMs: number
  /** Index in SIM_TRACKS of the currently loaded track */
  trackIndex: number
  pitchPhase: number
  swapping: boolean
  /** True while a random action temporarily owns the play state */
  busy: boolean
}

/**
 * Scripted engine for developing the UI without Pro DJ Link hardware:
 * two decks that play, drift in pitch, and chain generated tracks.
 */
export class SimEngine extends BaseEngine {
  private simDecks: SimDeck[] = []
  private masterDeck: DeckId = 1
  private timer: NodeJS.Timeout | null = null
  private nextTrackId = 100
  /** Rolling index into SIM_TRACKS so decks pick distinct tracks. */
  private trackCursor = 0

  constructor(private deckCount: number = 2) {
    super()
  }

  async start(): Promise<void> {
    this.updateNetworkState('searching')
    this.simDecks = Array.from({ length: this.deckCount }, (_, i) =>
      this.createDeck(i + 1, this.trackCursor++ % SIM_TRACKS.length)
    )

    setTimeout(() => {
      this.updateNetworkState('connected')
      for (const deck of this.simDecks) {
        this.deviceConnected({
          deckId: deck.deckId,
          name: `SIM-XDJ deck ${deck.deckId}`,
          ip: `192.168.1.${10 + deck.deckId}`
        })
        this.updateDeckTrack(deck.deckId, deck.track)
      }
      // Deck 1 is already rolling, the others join the mix staggered
      for (const [i, deck] of this.simDecks.entries()) {
        setTimeout(() => {
          deck.playState = 'playing'
        }, i * 5000)
      }
      this.timer = setInterval(() => this.tick(), TICK_MS)
    }, 800)
  }

  async stop(): Promise<void> {
    if (this.timer) clearInterval(this.timer)
    this.timer = null
    this.updateNetworkState('offline')
  }

  private createDeck(deckId: DeckId, trackIndex: number): SimDeck {
    return {
      deckId,
      playState: 'cued',
      track: buildSimTrack(SIM_TRACKS[trackIndex], this.nextTrackId++),
      playheadMs: 0,
      trackIndex,
      pitchPhase: deckId * 2.1,
      swapping: false,
      busy: false
    }
  }

  private tick(): void {
    for (const deck of this.simDecks) {
      const pitch = this.pitchOf(deck)
      if (deck.playState === 'playing' && deck.track) {
        deck.playheadMs += TICK_MS * (1 + pitch / 100)
        const remaining = deck.track.durationMs - deck.playheadMs
        if (remaining <= TRACK_SWAP_MARGIN_MS && !deck.swapping) {
          this.scheduleTrackSwap(deck)
        }
        if (remaining <= 0) {
          deck.playheadMs = deck.track.durationMs
          deck.playState = 'ended'
        }
        this.maybeAct(deck)
      }
      this.emitDeckState(deck, pitch)
    }
  }

  /** Occasionally behaves like a DJ: pauses, jumps to a cue, scratches. */
  private maybeAct(deck: SimDeck): void {
    if (deck.busy || deck.swapping || !deck.track) return
    const roll = Math.random()

    if (roll < ACTION_ODDS.pause) {
      deck.busy = true
      deck.playState = 'paused'
      setTimeout(
        () => this.resume(deck),
        PAUSE_MIN_MS + Math.random() * (PAUSE_MAX_MS - PAUSE_MIN_MS)
      )
    } else if (roll < ACTION_ODDS.pause + ACTION_ODDS.cueJump) {
      // Jump to a random cue not too close to the end of the track
      const targets = deck.track.cues.filter(
        (cue) => cue.offsetMs < deck.track!.durationMs - TRACK_SWAP_MARGIN_MS * 2
      )
      if (targets.length > 0) {
        deck.playheadMs = targets[Math.floor(Math.random() * targets.length)].offsetMs
      }
    } else if (roll < ACTION_ODDS.pause + ACTION_ODDS.cueJump + ACTION_ODDS.scratch) {
      deck.busy = true
      deck.playState = 'searching'
      const origin = deck.playheadMs
      setTimeout(() => {
        // Scratch lands slightly off where it started
        deck.playheadMs = Math.max(0, origin + (Math.random() - 0.5) * 4000)
        this.resume(deck)
      }, SCRATCH_MS)
    }
  }

  private resume(deck: SimDeck): void {
    if (!deck.swapping && deck.track) deck.playState = 'playing'
    deck.busy = false
  }

  /** Slow sinusoidal pitch drift, ±3%, unique phase per deck. */
  private pitchOf(deck: SimDeck): number {
    deck.pitchPhase += TICK_MS / 30000
    return Math.sin(deck.pitchPhase) * 3
  }

  private scheduleTrackSwap(deck: SimDeck): void {
    deck.swapping = true
    // Hand master over to the other deck, as a DJ transition would
    const other = this.simDecks.find((d) => d.deckId !== deck.deckId)
    if (other && other.playState === 'playing') this.masterDeck = other.deckId

    setTimeout(() => {
      deck.trackIndex = this.trackCursor++ % SIM_TRACKS.length
      deck.playState = 'loading'
      deck.track = null
      deck.playheadMs = 0
      this.updateDeckTrack(deck.deckId, null)

      setTimeout(() => {
        deck.track = buildSimTrack(SIM_TRACKS[deck.trackIndex], this.nextTrackId++)
        deck.playState = 'cued'
        this.updateDeckTrack(deck.deckId, deck.track)

        setTimeout(() => {
          deck.playState = 'playing'
          deck.swapping = false
        }, CUE_TO_PLAY_DELAY_MS)
      }, TRACK_LOAD_DELAY_MS)
    }, TRACK_SWAP_MARGIN_MS)
  }

  private emitDeckState(deck: SimDeck, pitch: number): void {
    const bpm = deck.track?.bpm ?? null
    const beatMs = bpm ? 60000 / bpm : null
    this.updateDeckState({
      deckId: deck.deckId,
      playState: deck.playState,
      isMaster: this.masterDeck === deck.deckId && deck.playState === 'playing',
      isSynced: deck.deckId !== this.masterDeck && deck.playState === 'playing',
      isOnAir: deck.playState === 'playing',
      pitch,
      trackBpm: bpm,
      effectiveBpm: bpm ? bpm * (1 + pitch / 100) : null,
      beatInMeasure:
        beatMs && deck.playState === 'playing'
          ? (Math.floor(deck.playheadMs / beatMs) % 4) + 1
          : null,
      trackId: deck.track?.trackId ?? null,
      playheadMs: deck.track ? deck.playheadMs : null,
      reportedAt: Date.now()
    })
  }
}
