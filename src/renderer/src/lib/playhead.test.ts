import { describe, expect, it } from 'vitest'

import type { DeckState } from '../model'
import { estimatePlayheadMs, MAX_EXTRAPOLATION_MS } from './playhead'

function deck(overrides: Partial<DeckState> = {}): DeckState {
  return {
    deckId: 1,
    playState: 'playing',
    isMaster: false,
    isSynced: false,
    isOnAir: false,
    pitch: 0,
    trackBpm: null,
    effectiveBpm: null,
    beatInMeasure: null,
    trackId: null,
    playheadMs: 10_000,
    reportedAt: 1_000,
    activeLoop: null,
    ...overrides
  }
}

describe('estimatePlayheadMs', () => {
  it('projects a playing deck forward with its pitch', () => {
    // 500ms of wall time at +4% pitch.
    expect(estimatePlayheadMs(deck({ pitch: 4 }), 1_500)).toBe(10_520)
  })

  it('jumps back to the loop start when the projection runs past the loop end', () => {
    const looping = deck({ activeLoop: { range: { startMs: 9_000, endMs: 10_200 } } })
    // 500ms past 10_000 overshoots the end by 300ms.
    expect(estimatePlayheadMs(looping, 1_500)).toBe(9_300)
  })

  it('projects straight on when the loop bounds are unknown', () => {
    expect(estimatePlayheadMs(deck({ activeLoop: { range: null } }), 1_500)).toBe(10_500)
  })

  it('holds a paused deck where it was reported', () => {
    expect(estimatePlayheadMs(deck({ playState: 'paused' }), 60_000)).toBe(10_000)
  })

  it('holds a playing deck once its status stopped arriving', () => {
    const held = estimatePlayheadMs(deck(), 1_000 + MAX_EXTRAPOLATION_MS)
    // A minute without status: the deck may have stopped long ago.
    expect(estimatePlayheadMs(deck(), 61_000)).toBe(held)
  })
})
