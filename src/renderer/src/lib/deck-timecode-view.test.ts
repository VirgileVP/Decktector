import { describe, expect, it } from 'vitest'

import type { DeckMatchStatus, DeckResolution } from '../model'
import { deckTimecodeView, type DeckTimecodeRates } from './deck-timecode-view'

const RATES: DeckTimecodeRates = { showFps: 25, outputFps: 30 }
const FRAMES_PER_HOUR_AT_25 = 25 * 3600

function status(
  resolution: DeckResolution,
  startFrames: number | null = null,
  rowTitle: string | null = null
): DeckMatchStatus {
  return { deckId: 1, resolution, rowTitle, startFrames, positionQuality: 'beatGrid' }
}

describe('deckTimecodeView', () => {
  it('every resolution has a distinct text', () => {
    const resolutions: [DeckResolution, number | null, string | null][] = [
      [{ type: 'matched', rowId: 'a' }, FRAMES_PER_HOUR_AT_25 / 3, 'Strobe'],
      [{ type: 'pending' }, null, null],
      [{ type: 'rateMismatch' }, null, null],
      [{ type: 'fallback', reason: { type: 'noMatch' } }, 23 * FRAMES_PER_HOUR_AT_25, null],
      [{ type: 'fallback', reason: { type: 'noDuration' } }, 23 * FRAMES_PER_HOUR_AT_25, null],
      [
        { type: 'fallback', reason: { type: 'ambiguous', rowIds: ['a', 'b'] } },
        23 * FRAMES_PER_HOUR_AT_25,
        null
      ],
      [
        { type: 'fallback', reason: { type: 'blankStart', rowId: 'a' } },
        23 * FRAMES_PER_HOUR_AT_25,
        'Strobe'
      ],
      [{ type: 'fallback', reason: { type: 'pendingTimeout' } }, 23 * FRAMES_PER_HOUR_AT_25, null],
      [{ type: 'cut', reason: { type: 'noMatch' } }, null, null],
      [{ type: 'cut', reason: { type: 'pendingTimeout' } }, null, null]
    ]
    const texts = resolutions.map(
      ([resolution, startFrames, rowTitle]) =>
        deckTimecodeView(status(resolution, startFrames, rowTitle), RATES)?.text
    )
    expect(texts).toEqual([
      '▸ Strobe · 00:20:00:00',
      'identifying…',
      'TC cut · show 25 fps, output 30 fps',
      'fallback 23:00:00:00 · unknown track',
      'fallback 23:00:00:00 · no duration',
      'fallback 23:00:00:00 · ambiguous (2 rows)',
      'fallback 23:00:00:00 · not yet timed (blank start)',
      'fallback 23:00:00:00 · identification timed out',
      'TC cut · unknown track',
      'TC cut · identification timed out'
    ])
    expect(new Set(texts).size).toBe(texts.length)
  })

  it('an ambiguous deck names how many rows it matched', () => {
    const view = deckTimecodeView(
      status({ type: 'fallback', reason: { type: 'ambiguous', rowIds: ['a', 'b', 'c'] } }, 0),
      RATES
    )
    expect(view?.text).toContain('ambiguous (3 rows)')
  })

  it('a cut deck is marked as an alert tone not a warning', () => {
    expect(
      deckTimecodeView(status({ type: 'cut', reason: { type: 'noMatch' } }), RATES)?.tone
    ).toBe('alert')
    expect(
      deckTimecodeView(status({ type: 'fallback', reason: { type: 'noMatch' } }, 0), RATES)?.tone
    ).toBe('warning')
  })

  it('a rate mismatch is marked as an alert tone', () => {
    expect(deckTimecodeView(status({ type: 'rateMismatch' }), RATES)?.tone).toBe('alert')
  })

  it('a deck with no show renders no line', () => {
    expect(deckTimecodeView(status({ type: 'inactive' }), RATES)).toBeNull()
    expect(deckTimecodeView(status({ type: 'none' }), RATES)).toBeNull()
  })
})
