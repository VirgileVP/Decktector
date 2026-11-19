import { describe, expect, it } from 'vitest'

import type { DeckId, DeckMatchStatus, DeckResolution, PlayState } from '../model'
import { rowDecks } from './show-row-decks'

function match(deckId: DeckId, resolution: DeckResolution): DeckMatchStatus {
  return { deckId, resolution, rowTitle: null, startFrames: null, positionQuality: 'precise' }
}

const MATCHED_A: DeckResolution = { type: 'matched', rowId: 'a' }

describe('rowDecks', () => {
  it('a matched deck that is playing holds its row as playing', () => {
    const rows = rowDecks({ 2: match(2, MATCHED_A) }, { 2: 'playing' })
    expect(rows.get('a')).toEqual({ playing: [2], loaded: [] })
  })

  it('a matched deck that is not playing holds its row as loaded', () => {
    const states: PlayState[] = ['cued', 'paused', 'searching', 'ended', 'loading']
    for (const state of states) {
      expect(rowDecks({ 1: match(1, MATCHED_A) }, { 1: state }).get('a')).toEqual({
        playing: [],
        loaded: [1]
      })
    }
  })

  it('a deck off the show holds no row', () => {
    const fallback: DeckResolution = { type: 'fallback', reason: { type: 'noMatch' } }
    expect(rowDecks({ 1: match(1, fallback) }, { 1: 'playing' }).size).toBe(0)
  })

  it('decks on the same row are split by state, in deck order', () => {
    const rows = rowDecks(
      { 4: match(4, MATCHED_A), 3: match(3, MATCHED_A), 1: match(1, MATCHED_A) },
      { 1: 'playing', 3: 'cued', 4: 'playing' }
    )
    expect(rows.get('a')).toEqual({ playing: [1, 4], loaded: [3] })
  })
})
