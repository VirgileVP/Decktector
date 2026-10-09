import { describe, expect, it } from 'vitest'

import type { LineupData } from './lineup'
import {
  LINEUP_FILE_FORMAT,
  LINEUP_FILE_VERSION,
  lineupFileName,
  parseLineupFile,
  serializeLineup
} from './lineup-file'

const NIGHT: LineupData = {
  doorsOffset: 19 * 60,
  endOffset: 1440 + 4 * 60,
  slots: [
    { id: 'a', artist: 'Opener', note: 'Bring USB', startOffset: 20 * 60 },
    { id: 'b', artist: 'Closer', note: '', startOffset: 1440 + 2 * 60 },
    { id: 'c', artist: 'Unplaced', note: '', startOffset: null }
  ]
}

function fileWith(overrides: Record<string, unknown>): string {
  return JSON.stringify({
    format: LINEUP_FILE_FORMAT,
    version: LINEUP_FILE_VERSION,
    doorsOffset: null,
    endOffset: null,
    slots: [],
    ...overrides
  })
}

function errorOf(text: string): string {
  const result = parseLineupFile(text)
  if (result.ok) throw new Error('expected the file to be rejected')
  return result.error
}

describe('serializeLineup / parseLineupFile', () => {
  it('round-trips a night, giving the sets fresh ids', () => {
    const result = parseLineupFile(serializeLineup(NIGHT))
    if (!result.ok) throw new Error(result.error)

    expect(result.lineup.doorsOffset).toBe(NIGHT.doorsOffset)
    expect(result.lineup.endOffset).toBe(NIGHT.endOffset)
    expect(
      result.lineup.slots.map(({ artist, note, startOffset }) => [artist, note, startOffset])
    ).toEqual(NIGHT.slots.map(({ artist, note, startOffset }) => [artist, note, startOffset]))
    expect(result.lineup.slots.map((slot) => slot.id)).not.toContain('a')
    expect(new Set(result.lineup.slots.map((slot) => slot.id)).size).toBe(3)
  })

  it('leaves slot ids out of the file', () => {
    expect(serializeLineup(NIGHT)).not.toContain('"id"')
  })

  it('treats a missing note or marker as empty', () => {
    const result = parseLineupFile(
      JSON.stringify({
        format: LINEUP_FILE_FORMAT,
        version: LINEUP_FILE_VERSION,
        slots: [{ artist: 'Solo', startOffset: 600 }]
      })
    )
    if (!result.ok) throw new Error(result.error)

    expect(result.lineup.doorsOffset).toBeNull()
    expect(result.lineup.endOffset).toBeNull()
    expect(result.lineup.slots[0].note).toBe('')
  })
})

describe('parseLineupFile rejects', () => {
  it('text that is not JSON', () => {
    expect(errorOf('not json')).toMatch(/not valid JSON/)
  })

  it('JSON that is not a lineup file', () => {
    expect(errorOf('{"slots": []}')).toMatch(/not a Decktector lineup/)
    expect(errorOf('[]')).toMatch(/not a Decktector lineup/)
  })

  it('a file from a newer version', () => {
    expect(errorOf(fileWith({ version: LINEUP_FILE_VERSION + 1 }))).toMatch(/newer version/)
  })

  it('a time outside the two-day axis', () => {
    expect(errorOf(fileWith({ endOffset: 2 * 1440 + 1 }))).toMatch(/endOffset/)
    expect(errorOf(fileWith({ slots: [{ artist: 'X', startOffset: -5 }] }))).toMatch(/Set 1/)
    expect(errorOf(fileWith({ slots: [{ artist: 'X', startOffset: 10.5 }] }))).toMatch(/Set 1/)
  })

  it('a set without an artist', () => {
    expect(errorOf(fileWith({ slots: [{ startOffset: 600 }] }))).toMatch(/Set 1 has no artist/)
  })

  it('an absurd number of sets', () => {
    const slots = Array.from({ length: 501 }, () => ({ artist: 'X', startOffset: null }))
    expect(errorOf(fileWith({ slots }))).toMatch(/too many sets/)
  })
})

describe('lineupFileName', () => {
  it('dates the file with the local day', () => {
    expect(lineupFileName(new Date(2026, 9, 9, 23, 30))).toBe('lineup-2026-10-09.json')
  })
})
