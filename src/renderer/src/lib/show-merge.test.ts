import { describe, expect, it } from 'vitest'

import type { ShowRow } from './show'
import { mergeRows, replaceRows, type ImportedRow } from './show-merge'

function imported(title: string, durationSeconds = 300, artist = 'Artist'): ImportedRow {
  return { title, artist, durationSeconds }
}

function existingRow(id: string, title: string, overrides: Partial<ShowRow> = {}): ShowRow {
  return {
    id,
    title,
    artist: 'Artist',
    durationSeconds: 300,
    startTimecode: null,
    trimMs: 0,
    absent: false,
    ...overrides
  }
}

describe('mergeRows', () => {
  it('merge keeps start and trim of a matched row', () => {
    const kept = existingRow('kept', 'Strobe', {
      startTimecode: '01:20:00:00',
      trimMs: 80,
      setArtist: 'DJ Example'
    })
    const [merged] = mergeRows([kept], [imported('Strobe')])
    expect(merged).toBe(kept)
  })

  it('merge adds unmatched imported rows blank', () => {
    const merged = mergeRows([existingRow('kept', 'Strobe')], [imported('Strobe'), imported('New')])
    expect(merged[1]).toMatchObject({
      title: 'New',
      startTimecode: null,
      trimMs: 0,
      absent: false
    })
    expect(merged[1].id).not.toBe('kept')
  })

  it('merge flags rows missing from the playlist without deleting them', () => {
    const dropped = existingRow('dropped', 'Gone', { startTimecode: '02:00:00:00' })
    const merged = mergeRows([dropped], [imported('Other')])
    expect(merged).toHaveLength(2)
    expect(merged[1]).toEqual({ ...dropped, absent: true })
  })

  it('merge pairs each existing row once for a track listed twice', () => {
    const first = existingRow('first', 'Twice', { startTimecode: '01:00:00:00' })
    const second = existingRow('second', 'Twice', { startTimecode: '03:00:00:00' })
    const merged = mergeRows(
      [first, second],
      [imported('Twice'), imported('Twice'), imported('Twice')]
    )
    expect(merged.slice(0, 2)).toEqual([first, second])
    expect(merged[2]).toMatchObject({ title: 'Twice', startTimecode: null })
    expect(merged).toHaveLength(3)
  })

  it('merge matches through normalisation and the 2s window', () => {
    const row = existingRow('row', 'Foo (feat. X)', { durationSeconds: 300 })
    const near = mergeRows([row], [imported('FOO (Feat. x)', 301)])
    expect(near).toEqual([row])

    const far = mergeRows([row], [imported('FOO (Feat. x)', 303)])
    expect(far).toHaveLength(2)
    expect(far[1]).toEqual({ ...row, absent: true })
  })

  it('a previously absent row that returns is no longer absent', () => {
    const returning = existingRow('back', 'Back', { absent: true, startTimecode: '04:00:00:00' })
    expect(mergeRows([returning], [imported('Back')])).toEqual([{ ...returning, absent: false }])
  })

  it('merge orders playlist rows first then absent rows in their previous order', () => {
    const rows = [
      existingRow('a', 'A'),
      existingRow('b', 'B'),
      existingRow('c', 'C'),
      existingRow('d', 'D')
    ]
    const merged = mergeRows(rows, [imported('C'), imported('New'), imported('A')])
    expect(merged.map((row) => row.title)).toEqual(['C', 'New', 'A', 'B', 'D'])
    expect(merged.map((row) => row.absent)).toEqual([false, false, false, true, true])
  })

  it('merge keeps the existing title artist and duration strings', () => {
    const row = existingRow('row', 'Café del Mar', { artist: 'Energy 52', durationSeconds: 480 })
    const [merged] = mergeRows([row], [imported('CAFÉ DEL MAR', 481, 'energy 52')])
    expect(merged).toMatchObject({
      title: 'Café del Mar',
      artist: 'Energy 52',
      durationSeconds: 480
    })
  })

  it('merge into an empty show equals replace', () => {
    const playlist = [imported('One'), imported('Two', 0)]
    const withoutIds = (rows: ShowRow[]): Partial<ShowRow>[] =>
      rows.map((row) => ({ ...row, id: undefined }))
    expect(withoutIds(mergeRows([], playlist))).toEqual(withoutIds(replaceRows(playlist)))
  })
})

describe('replaceRows', () => {
  it('replace discards everything and issues fresh ids', () => {
    const rows = replaceRows([imported('One'), imported('Two')])
    expect(
      rows.map(({ title, startTimecode, trimMs, absent }) => [title, startTimecode, trimMs, absent])
    ).toEqual([
      ['One', null, 0, false],
      ['Two', null, 0, false]
    ])
    expect(new Set(rows.map((row) => row.id)).size).toBe(2)
    expect(rows.every((row) => row.setArtist === undefined)).toBe(true)
  })
})
