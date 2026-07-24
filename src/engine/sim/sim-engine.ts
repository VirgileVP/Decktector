import { BaseEngine } from '../engine'
import type { DeckId, PlayState, TrackInfo } from '../types'
import { SIM_TRACKS, buildSimTrack } from './sim-tracks'

const TICK_MS = 200
/** Time before the end of a track at which the sim deck loads the next one. */
const TRACK_SWAP_MARGIN_MS = 8000
const TRACK_LOAD_DELAY_MS = 1500
const CUE_TO_PLAY_DELAY_MS = 2500

interface SimDeck {
  deckId: DeckId
  playState: PlayState
  track: TrackInfo | null
  playheadMs: number
  /** Index in SIM_TRACKS of the currently loaded track */
  trackIndex: number
  pitchPhase: number
  swapping: boolean
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
      swapping: false
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
      }
      this.emitDeckState(deck, pitch)
    }
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
