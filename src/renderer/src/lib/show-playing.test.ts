import { describe, expect, it } from 'vitest'

import type { DeckId, DeckMatchStatus, DeckResolution, PlayState } from '../model'
import { playingRows } from './show-playing'

function match(deckId: DeckId, resolution: DeckResolution): DeckMatchStatus {
  return { deckId, resolution, rowTitle: null, startFrames: null, positionQuality: 'precise' }
}

const MATCHED_A: DeckResolution = { type: 'matched', rowId: 'a' }

describe('playingRows', () => {
  it('a matched deck that is playing marks its row', () => {
    const rows = playingRows({ 2: match(2, MATCHED_A) }, { 2: 'playing' })
    expect([...rows]).toEqual([['a', [2]]])
  })

  it('a matched deck that is not playing marks nothing', () => {
    const states: PlayState[] = ['cued', 'paused', 'searching', 'ended', 'loading', 'empty']
    for (const state of states) {
      expect(playingRows({ 1: match(1, MATCHED_A) }, { 1: state }).size).toBe(0)
    }
  })

  it('a playing deck off the show marks nothing', () => {
    const fallback: DeckResolution = { type: 'fallback', reason: { type: 'noMatch' } }
    expect(playingRows({ 1: match(1, fallback) }, { 1: 'playing' }).size).toBe(0)
  })

  it('two decks playing the same row are both listed in deck order', () => {
    const rows = playingRows(
      { 3: match(3, MATCHED_A), 1: match(1, MATCHED_A) },
      { 1: 'playing', 3: 'playing' }
    )
    expect(rows.get('a')).toEqual([1, 3])
  })
})
