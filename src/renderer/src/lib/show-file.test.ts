import { describe, expect, it } from 'vitest'

import type { ShowData, ShowRow } from './show'
import {
  MAX_IMPORTED_ROWS,
  MAX_TEXT_LENGTH,
  SHOW_FILE_FORMAT,
  SHOW_FILE_VERSION,
  parseShowFile,
  serializeShow,
  showFileName
} from './show-file'
import { analyseShow } from './show-validation'
import { SHOW_FPS_OPTIONS } from './show-timecode'

const NIGHT: ShowData = {
  fps: 25,
  fallbackStartTimecode: '23:00:00:00',
  cutWhenUnidentified: true,
  rows: [
    {
      id: 'a',
      title: 'Strobe',
      artist: 'deadmau5',
      durationSeconds: 637,
      startTimecode: '01:20:00:00',
      trimMs: 80,
      setArtist: 'DJ Example',
      absent: false
    },
    {
      id: 'b',
      title: 'Unknown length',
      artist: '',
      durationSeconds: 0,
      startTimecode: null,
      trimMs: -40,
      absent: true
    }
  ]
}

const VALID_ROW = {
  title: 'Strobe',
  artist: 'deadmau5',
  durationSeconds: 637,
  startTc: '01:20:00:00',
  trimMs: 0
}

function fileWith(overrides: Record<string, unknown>): string {
  return JSON.stringify({
    format: SHOW_FILE_FORMAT,
    version: SHOW_FILE_VERSION,
    fps: 25,
    fallbackStartTc: '23:00:00:00',
    cutWhenUnidentified: false,
    rows: [],
    ...overrides
  })
}

function fileWithRow(overrides: Record<string, unknown>): string {
  return fileWith({ rows: [{ ...VALID_ROW, ...overrides }] })
}

function parsed(text: string): ShowData {
  const result = parseShowFile(text)
  if (!result.ok) throw new Error(result.error)
  return result.show
}

function errorOf(text: string): string {
  const result = parseShowFile(text)
  if (result.ok) throw new Error('expected the file to be rejected')
  return result.error
}

const withoutIds = (rows: ShowRow[]): Partial<ShowRow>[] =>
  rows.map((row) => ({ ...row, id: undefined }))

describe('serializeShow / parseShowFile', () => {
  it('a show round trips through its file', () => {
    const show = parsed(serializeShow(NIGHT))
    expect({ ...show, rows: withoutIds(show.rows) }).toEqual({
      ...NIGHT,
      rows: withoutIds(NIGHT.rows)
    })
  })

  it('ids are fresh after parsing and left out of the file', () => {
    const text = serializeShow(NIGHT)
    expect(text).not.toContain('"id"')
    const ids = parsed(text).rows.map((row) => row.id)
    expect(ids).not.toContain('a')
    expect(new Set(ids).size).toBe(2)
  })

  it('a start timecode stored as a label round trips through the file', () => {
    for (const fps of SHOW_FPS_OPTIONS) {
      const label = `12:34:56:${String(fps - 1).padStart(2, '0')}`
      const show: ShowData = { ...NIGHT, fps, rows: [{ ...NIGHT.rows[0], startTimecode: label }] }
      const text = serializeShow(show)
      expect(JSON.parse(text).rows[0].startTc).toBe(label)
      expect(parsed(text).rows[0].startTimecode).toBe(label)
      // Nothing in the file is a frame count: 12:34:56 at any rate is far above 45 000.
      expect(text).not.toMatch(/\d{5,}/)
    }
  })

  it('a row with a null start stays blank', () => {
    expect(parsed(fileWithRow({ startTc: null })).rows[0].startTimecode).toBeNull()
    expect(parsed(fileWithRow({ startTc: undefined })).rows[0].startTimecode).toBeNull()
  })

  it('set artist is optional and never required', () => {
    expect(parsed(fileWithRow({})).rows[0].setArtist).toBeUndefined()
    expect(parsed(fileWithRow({ setArtist: 'DJ Example' })).rows[0].setArtist).toBe('DJ Example')
  })

  it('an unknown extra field is ignored', () => {
    const show = parsed(
      JSON.stringify({
        ...JSON.parse(fileWithRow({ performerId: 7 })),
        mergedFrom: ['dj-a.json', 'dj-b.json']
      })
    )
    expect(show.rows).toHaveLength(1)
  })

  it('a frame number at or above fps is kept not rejected', () => {
    const show = parsed(fileWithRow({ startTc: '00:00:00:28' }))
    expect(show.rows[0].startTimecode).toBe('00:00:00:28')
    expect(analyseShow(show).byRow.get(show.rows[0].id)).toContainEqual({ kind: 'invalidFrames' })
  })

  it('an absent flag defaults to false', () => {
    expect(parsed(fileWithRow({})).rows[0].absent).toBe(false)
  })
})

describe('parseShowFile rejects', () => {
  it('invalid json', () => {
    expect(errorOf('not json')).toMatch(/not valid JSON/)
  })

  it('not a show file', () => {
    expect(errorOf('{"rows": []}')).toMatch(/not a Decktector show/)
    expect(errorOf(JSON.stringify({ format: 'decktector.lineup', version: 1 }))).toMatch(
      /not a Decktector show/
    )
  })

  it('a newer version asks to update the app', () => {
    expect(errorOf(fileWith({ version: SHOW_FILE_VERSION + 1 }))).toMatch(/newer version/)
  })

  it('an unknown version is refused', () => {
    expect(errorOf(fileWith({ version: 0 }))).toMatch(/unknown version/)
    expect(errorOf(fileWith({ version: '1' }))).toMatch(/unknown version/)
  })

  it('a malformed timecode is rejected', () => {
    for (const startTc of ['25:00:00:00', '00:60:00:00', '1:2:3', 4500]) {
      expect(errorOf(fileWithRow({ startTc }))).toMatch(/Row 1 start is not a valid/)
    }
    expect(errorOf(fileWith({ fallbackStartTc: '24:00:00:00' }))).toMatch(/fallback start/)
  })

  it('a frame rate outside 24, 25 and 30', () => {
    expect(errorOf(fileWith({ fps: 29.97 }))).toMatch(/frame rate/)
  })

  it('too many rows are refused', () => {
    const rows = Array.from({ length: MAX_IMPORTED_ROWS + 1 }, () => VALID_ROW)
    expect(errorOf(fileWith({ rows }))).toMatch(/too many rows/)
  })

  it('a title over 300 chars is refused', () => {
    expect(errorOf(fileWithRow({ title: 'x'.repeat(MAX_TEXT_LENGTH + 1) }))).toMatch(
      /Row 1 title is too long/
    )
    expect(errorOf(fileWithRow({ title: '   ' }))).toMatch(/Row 1 title is empty/)
  })

  it('a trim beyond 10s is refused', () => {
    expect(parsed(fileWithRow({ trimMs: -10_000 })).rows[0].trimMs).toBe(-10_000)
    expect(errorOf(fileWithRow({ trimMs: 10_001 }))).toMatch(/Row 1 trim/)
  })

  it('a non integer duration is refused', () => {
    expect(errorOf(fileWithRow({ durationSeconds: 637.5 }))).toMatch(/Row 1 duration/)
    expect(errorOf(fileWithRow({ durationSeconds: -1 }))).toMatch(/Row 1 duration/)
  })

  it('a missing cut switch', () => {
    expect(errorOf(fileWith({ cutWhenUnidentified: undefined }))).toMatch(/cut unidentified/)
  })
})

describe('showFileName', () => {
  it('names the export after the local day', () => {
    expect(showFileName(new Date(2026, 9, 9, 23, 30))).toBe('show-2026-10-09.dshow')
  })
})
