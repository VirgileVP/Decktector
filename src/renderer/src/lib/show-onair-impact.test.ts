import { describe, expect, it } from 'vitest'

import type { ShowData, ShowRow } from './show'
import { onAirImpact, rowOfResolution, type DeckForImpact } from './show-onair-impact'

const STROBE = { title: 'Strobe', artist: 'deadmau5', durationSeconds: 637 }

function row(id: string, startTimecode: string | null, trimMs = 0): ShowRow {
  return { id, ...STROBE, startTimecode, trimMs, absent: false }
}

function show(rows: ShowRow[], fallbackStartTimecode = '23:00:00:00'): ShowData {
  return { fps: 25, fallbackStartTimecode, cutWhenUnidentified: false, rows }
}

function deck(onAir: boolean, track: DeckForImpact['track'] = STROBE): DeckForImpact {
  return { deckId: 2, track, onAir }
}

const MATCHED = { 2: { rowId: 'strobe' } }

describe('onAirImpact', () => {
  it('a trim change is never an impact', () => {
    const before = show([row('strobe', '01:00:00:00')])
    const after = show([row('strobe', '01:00:00:00', 120)])
    expect(onAirImpact(before, after, [deck(true)], MATCHED)).toEqual([])
  })

  it('a start change on an on air matched deck is an impact', () => {
    const before = show([row('strobe', '01:00:00:00')])
    const after = show([row('strobe', '02:00:00:00')])
    expect(onAirImpact(before, after, [deck(true)], MATCHED)).toEqual([
      { deckId: 2, before: '01:00:00:00', after: '02:00:00:00' }
    ])
  })

  it('a start change on a deck that is not on air is not', () => {
    const before = show([row('strobe', '01:00:00:00')])
    const after = show([row('strobe', '02:00:00:00')])
    expect(onAirImpact(before, after, [deck(false)], MATCHED)).toEqual([])
  })

  it('replacing the show under an on air deck reports its old and new timecode', () => {
    const before = show([row('strobe', '01:00:00:00')])
    // Replace issues new ids and blank starts: the deck falls back.
    const replaced = show([row('fresh-id', null)])
    expect(onAirImpact(before, replaced, [deck(true)], MATCHED)).toEqual([
      { deckId: 2, before: '01:00:00:00', after: '23:00:00:00' }
    ])
    // A new row with a start for the same track is where the deck goes.
    const timed = show([row('fresh-id', '05:00:00:00')])
    expect(onAirImpact(before, timed, [deck(true)], MATCHED)).toEqual([
      { deckId: 2, before: '01:00:00:00', after: '05:00:00:00' }
    ])
    // Unloading the show takes it back to midnight.
    expect(onAirImpact(before, null, [deck(true)], MATCHED)[0].after).toBe('00:00:00:00')
  })

  it('a deck resolved to the fallback changes when the fallback changes', () => {
    const before = show([], '23:00:00:00')
    const after = show([], '22:00:00:00')
    expect(onAirImpact(before, after, [deck(true)], { 2: { rowId: null } })).toEqual([
      { deckId: 2, before: '23:00:00:00', after: '22:00:00:00' }
    ])
  })

  it('typing the start of the blank row a deck waits on is an impact', () => {
    const before = show([row('strobe', null)])
    const after = show([row('strobe', '01:00:00:00')])
    expect(onAirImpact(before, after, [deck(true)], MATCHED)).toEqual([
      { deckId: 2, before: '23:00:00:00', after: '01:00:00:00' }
    ])
  })

  it('no decks loaded means no impact', () => {
    const before = show([row('strobe', '01:00:00:00')])
    expect(onAirImpact(before, null, [], {})).toEqual([])
    expect(onAirImpact(before, null, [deck(true, null)], {})).toEqual([])
  })
})

describe('rowOfResolution', () => {
  it('names the matched row, or the blank row a deck waits on', () => {
    expect(rowOfResolution({ type: 'matched', rowId: 'a' })).toBe('a')
    expect(rowOfResolution({ type: 'fallback', reason: { type: 'blankStart', rowId: 'b' } })).toBe(
      'b'
    )
    expect(rowOfResolution({ type: 'cut', reason: { type: 'noMatch' } })).toBeNull()
    expect(rowOfResolution({ type: 'pending' })).toBeNull()
  })
})
