import { describe, expect, it } from 'vitest'

import type { DeckState, PlayState } from '../model'
import {
  effectiveHandover,
  onAirDeckIds,
  parseTimecodeHandover,
  pickOnAirTimecodeSource
} from './timecode-follow'

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

describe('effectiveHandover', () => {
  it('newOnAir is forced to oldLeaves when the source is playback', () => {
    expect(effectiveHandover('newOnAir', 'playback', 1)).toBe('oldLeaves')
  })

  it('newOnAir is forced to oldLeaves when auto resolves to playback with two mixers', () => {
    expect(effectiveHandover('newOnAir', 'auto', 2)).toBe('oldLeaves')
  })

  it('newOnAir is honoured when auto resolves to a single mixer', () => {
    expect(effectiveHandover('newOnAir', 'auto', 1)).toBe('newOnAir')
  })

  it('oldLeaves stays oldLeaves whatever the source', () => {
    expect(effectiveHandover('oldLeaves', 'mixer', 1)).toBe('oldLeaves')
  })
})

describe('parseTimecodeHandover', () => {
  it('keeps a known handover and defaults anything else to oldLeaves', () => {
    expect(parseTimecodeHandover('newOnAir')).toBe('newOnAir')
    for (const raw of [undefined, null, 'later', 3]) {
      expect(parseTimecodeHandover(raw)).toBe('oldLeaves')
    }
  })
})

describe('pickOnAirTimecodeSource with oldLeaves', () => {
  const pick = (onAirIds: number[], currentSource: number | null): number | null =>
    pickOnAirTimecodeSource(onAirIds, onAirIds, currentSource, 'oldLeaves')

  it('leaves the source alone when no deck is live', () => {
    expect(pick([], 2)).toBeNull()
    expect(pick([], null)).toBeNull()
  })

  it('keeps the current source while it is still live', () => {
    expect(pick([1, 2], 2)).toBeNull()
  })

  it('moves to the live deck when the current source went off air', () => {
    expect(pick([3], 2)).toBe(3)
  })

  it('takes the lowest live deck when there is no current source', () => {
    expect(pick([2, 4], null)).toBe(2)
  })

  it('ignores a rising edge while the current source is still live', () => {
    expect(pickOnAirTimecodeSource([1, 2], [1], 1, 'oldLeaves')).toBeNull()
  })
})

describe('pickOnAirTimecodeSource with newOnAir', () => {
  it('a rising edge takes the source while the old deck is still on air', () => {
    expect(pickOnAirTimecodeSource([1, 2], [1], 1, 'newOnAir')).toBe(2)
  })

  it('two decks rising together hand over to the lowest deck number', () => {
    expect(pickOnAirTimecodeSource([1, 3, 4], [1], 1, 'newOnAir')).toBe(3)
  })

  it('no rising edge keeps the current source while it is live', () => {
    expect(pickOnAirTimecodeSource([1, 2], [1, 2], 1, 'newOnAir')).toBeNull()
  })

  it('a crossfade does not flip the source back and forth', () => {
    // Deck 2 comes up while deck 1 is on air: the timecode moves to deck 2.
    expect(pickOnAirTimecodeSource([1, 2], [1], 1, 'newOnAir')).toBe(2)
    // Both stay up through the crossfade: nothing moves.
    expect(pickOnAirTimecodeSource([1, 2], [1, 2], 2, 'newOnAir')).toBeNull()
    // Deck 1 goes down: deck 2 keeps the timecode.
    expect(pickOnAirTimecodeSource([2], [1, 2], 2, 'newOnAir')).toBeNull()
  })

  it('a deck rising that already holds the source leaves it alone', () => {
    expect(pickOnAirTimecodeSource([2], [], 2, 'newOnAir')).toBeNull()
  })

  it('falls back to oldLeaves when the source leaves with no deck rising', () => {
    expect(pickOnAirTimecodeSource([3], [2, 3], 2, 'newOnAir')).toBe(3)
  })
})
