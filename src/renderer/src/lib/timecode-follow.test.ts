import { describe, expect, it } from 'vitest'

import type { DeckState, PlayState } from '../model'
import { onAirDeckIds, pickOnAirTimecodeSource } from './timecode-follow'

function deck(deckId: number, playState: PlayState, isOnAir: boolean): DeckState {
  return { deckId, playState, isOnAir } as DeckState
}

describe('onAirDeckIds', () => {
  const decks = {
    1: deck(1, 'playing', false),
    2: deck(2, 'cued', true),
    3: deck(3, 'playing', true)
  }

  it('follows the mixer flag with one mixer', () => {
    expect(onAirDeckIds(decks, 'auto', 1)).toEqual([2, 3])
  })

  it('follows playback without a mixer', () => {
    expect(onAirDeckIds(decks, 'auto', 0)).toEqual([1, 3])
  })

  it('returns nothing when no deck is live', () => {
    expect(onAirDeckIds({ 1: deck(1, 'paused', false) }, 'playback', 0)).toEqual([])
  })
})

describe('pickOnAirTimecodeSource', () => {
  it('leaves the source alone when no deck is live', () => {
    expect(pickOnAirTimecodeSource([], 2)).toBeNull()
    expect(pickOnAirTimecodeSource([], null)).toBeNull()
  })

  it('keeps the current source while it is still live', () => {
    expect(pickOnAirTimecodeSource([1, 2], 2)).toBeNull()
  })

  it('moves to the live deck when the current source went off air', () => {
    expect(pickOnAirTimecodeSource([3], 2)).toBe(3)
  })

  it('takes the lowest live deck when there is no current source', () => {
    expect(pickOnAirTimecodeSource([2, 4], null)).toBe(2)
  })
})
