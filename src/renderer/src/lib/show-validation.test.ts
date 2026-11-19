import { describe, expect, it } from 'vitest'

import type { ShowData, ShowRow } from './show'
import {
  analyseShow,
  fillBlanks,
  fpsChangeImpact,
  respaceStarts,
  mergeSameStartDuplicates,
  type RowIssue
} from './show-validation'
import type { ShowFps } from './show-timecode'

const FRAMES_PER_MINUTE_AT_25 = 60 * 25

function row(id: string, overrides: Partial<ShowRow> = {}): ShowRow {
  return {
    id,
    title: `Track ${id}`,
    artist: 'Artist',
    durationSeconds: 300,
    startTimecode: null,
    trimMs: 0,
    absent: false,
    ...overrides
  }
}

function show(rows: ShowRow[], fps: ShowFps = 25): ShowData {
  return { fps, fallbackStartTimecode: '23:00:00:00', cutWhenUnidentified: false, rows }
}

function kindsOf(issues: RowIssue[] | undefined): RowIssue['kind'][] {
  return (issues ?? []).map((issue) => issue.kind)
}

describe('analyseShow ranges', () => {
  it('ranges that touch end to start do not overlap', () => {
    const analysis = analyseShow(
      show([
        row('a', { startTimecode: '00:00:00:00', durationSeconds: 600 }),
        row('b', { startTimecode: '00:10:00:00' })
      ])
    )
    expect(analysis.byRow.size).toBe(0)
    expect(analysis.ready).toBe(true)
  })

  it('one frame of overlap is flagged', () => {
    const analysis = analyseShow(
      show([
        row('a', { startTimecode: '00:00:00:00', durationSeconds: 600 }),
        row('b', { startTimecode: '00:09:59:24' })
      ])
    )
    expect(analysis.byRow.get('a')).toEqual([{ kind: 'overlap', withRowId: 'b' }])
    expect(analysis.byRow.get('b')).toEqual([{ kind: 'overlap', withRowId: 'a' }])
    expect(analysis.conflicts).toBe(1)
  })

  it('a range ending exactly at 24h crosses midnight', () => {
    const ending = analyseShow(
      show([row('a', { startTimecode: '23:50:00:00', durationSeconds: 600 })])
    )
    expect(kindsOf(ending.byRow.get('a'))).toEqual(['crossesMidnight'])

    const shortOfIt = analyseShow(
      show([row('a', { startTimecode: '23:50:00:00', durationSeconds: 599 })])
    )
    expect(shortOfIt.byRow.get('a')).toBeUndefined()
  })

  it('a range that wraps overlaps a row at the start of the day', () => {
    const analysis = analyseShow(
      show([
        row('late', { startTimecode: '23:55:00:00', durationSeconds: 600 }),
        row('early', { startTimecode: '00:02:00:00' })
      ])
    )
    expect(kindsOf(analysis.byRow.get('late'))).toEqual(['crossesMidnight', 'overlap'])
    expect(analysis.byRow.get('early')).toEqual([{ kind: 'overlap', withRowId: 'late' }])
  })

  it('a row with unknown duration never reports an overlap', () => {
    const analysis = analyseShow(
      show([
        row('a', { startTimecode: '00:00:00:00', durationSeconds: 600 }),
        row('unknown', { startTimecode: '00:05:00:00', durationSeconds: 0 }),
        row('later', { startTimecode: '00:10:00:00' })
      ])
    )
    expect(analysis.byRow.size).toBe(0)
  })
})

describe('analyseShow readiness', () => {
  it('overlap blocks ready', () => {
    const analysis = analyseShow(
      show([row('a', { startTimecode: '00:00:00:00' }), row('b', { startTimecode: '00:01:00:00' })])
    )
    expect(analysis.ready).toBe(false)
  })

  it('midnight crossing blocks ready', () => {
    const analysis = analyseShow(show([row('a', { startTimecode: '23:59:00:00' })]))
    expect(analysis.conflicts).toBe(1)
    expect(analysis.ready).toBe(false)
  })

  it('invalid frames block ready', () => {
    const analysis = analyseShow(show([row('a', { startTimecode: '00:00:00:28' })], 25))
    expect(kindsOf(analysis.byRow.get('a'))).toEqual(['invalidFrames'])
    expect(analysis.ready).toBe(false)
  })

  it('a blank start does not block ready', () => {
    const analysis = analyseShow(
      show([row('a'), row('b', { absent: true, startTimecode: '01:00:00:00' })])
    )
    expect(kindsOf(analysis.byRow.get('a'))).toEqual(['blankStart'])
    expect(kindsOf(analysis.byRow.get('b'))).toEqual(['absent'])
    expect(analysis.warnings).toBe(2)
    expect(analysis.conflicts).toBe(0)
    expect(analysis.ready).toBe(true)
  })
})

describe('duplicates', () => {
  it('duplicates with the same start are merged and not a conflict', () => {
    const rows = [
      row('first', { title: 'Strobe', startTimecode: '01:00:00:00', trimMs: 80 }),
      row('copy', { title: 'Strobe', startTimecode: '01:00:00:00', trimMs: -40 })
    ]
    const analysis = analyseShow(show(rows))
    expect(analysis.byRow.get('first')).toEqual([
      { kind: 'duplicate', groupSize: 2, sameStart: true }
    ])
    expect(analysis.conflicts).toBe(0)
    expect(analysis.ready).toBe(true)

    const merged = mergeSameStartDuplicates(rows)
    expect(merged.map((kept) => [kept.id, kept.trimMs])).toEqual([['first', 80]])
    expect(analyseShow(show(merged)).byRow.size).toBe(0)
  })

  it('duplicates with different starts are a conflict and the show is not ready', () => {
    const rows = [
      row('first', { title: 'Strobe', startTimecode: '01:00:00:00' }),
      row('copy', { title: 'Strobe', startTimecode: '02:00:00:00' }),
      row('blank', { title: 'Strobe' })
    ]
    const analysis = analyseShow(show(rows))
    expect(analysis.byRow.get('first')).toEqual([
      { kind: 'duplicate', groupSize: 3, sameStart: false }
    ])
    expect(analysis.conflicts).toBe(1)
    expect(analysis.ready).toBe(false)
    expect(mergeSameStartDuplicates(rows)).toEqual(rows)
  })

  it('duplicates found through normalisation not exact text', () => {
    const analysis = analyseShow(
      show([
        row('a', { title: 'Song (Feat. X)', artist: 'Ärger', startTimecode: '01:00:00:00' }),
        row('b', { title: '  song (feat. x)', artist: 'ärger', durationSeconds: 302 })
      ])
    )
    expect(kindsOf(analysis.byRow.get('a'))).toEqual(['duplicate'])
  })

  it('a chain of near durations is one group', () => {
    const analysis = analyseShow(
      show([
        row('a', { title: 'Same', durationSeconds: 300 }),
        row('b', { title: 'Same', durationSeconds: 302 }),
        row('c', { title: 'Same', durationSeconds: 304 }),
        row('far', { title: 'Same', durationSeconds: 310 })
      ])
    )
    expect(analysis.byRow.get('c')).toContainEqual({
      kind: 'duplicate',
      groupSize: 3,
      sameStart: false
    })
    expect(kindsOf(analysis.byRow.get('far'))).toEqual(['blankStart'])
  })
})

describe('respaceStarts', () => {
  const halfHour = 30 * FRAMES_PER_MINUTE_AT_25

  it('renumbers every row from the first start, typed starts included', () => {
    const respaced = respaceStarts(
      [
        row('first', { startTimecode: '01:00:00:00' }),
        row('typed', { startTimecode: '01:03:00:00' }),
        row('blank')
      ],
      halfHour,
      25
    )
    expect(respaced.map((respacedRow) => respacedRow.startTimecode)).toEqual([
      '01:00:00:00',
      '01:30:00:00',
      '02:00:00:00'
    ])
  })

  it('starts at midnight when the first start is blank or does not parse', () => {
    for (const firstStart of [null, '00:00:00:28']) {
      const respaced = respaceStarts(
        [row('first', { startTimecode: firstStart }), row('second')],
        halfHour,
        25
      )
      expect(respaced.map((respacedRow) => respacedRow.startTimecode)).toEqual([
        '00:00:00:00',
        '00:30:00:00'
      ])
    }
  })

  it('wraps at 24 hours', () => {
    const respaced = respaceStarts(
      [row('late', { startTimecode: '23:30:00:00' }), row('after midnight')],
      2 * halfHour,
      25
    )
    expect(respaced[1].startTimecode).toBe('00:30:00:00')
  })
})

describe('fillBlanks', () => {
  const tenMinutes = 10 * FRAMES_PER_MINUTE_AT_25

  it('steps from the previous row', () => {
    const filled = fillBlanks(
      [
        row('first'),
        row('typed', { startTimecode: '01:00:00:00' }),
        row('next'),
        row('invalid', { startTimecode: '02:00:00:28' }),
        row('after invalid')
      ],
      tenMinutes,
      25
    )
    expect(filled.map((filledRow) => filledRow.startTimecode)).toEqual([
      '00:00:00:00',
      '01:00:00:00',
      '01:10:00:00',
      '02:00:00:28',
      '01:20:00:00'
    ])
  })

  it('leaves typed values alone', () => {
    const rows = [
      row('a', { startTimecode: '05:00:00:00' }),
      row('b', { startTimecode: '01:00:00:00' })
    ]
    expect(fillBlanks(rows, tenMinutes, 25)).toEqual(rows)
  })

  it('wraps at 24 hours', () => {
    const filled = fillBlanks(
      [row('a', { startTimecode: '23:55:00:00' }), row('b')],
      tenMinutes,
      25
    )
    expect(filled[1].startTimecode).toBe('00:05:00:00')
  })

  it('fill blanks overlaps it creates are flagged and block ready', () => {
    const rows = [
      row('a', { startTimecode: '00:00:00:00', durationSeconds: 900 }),
      row('b', { durationSeconds: 900 })
    ]
    const analysis = analyseShow(show(fillBlanks(rows, tenMinutes, 25)))
    expect(analysis.byRow.get('b')).toEqual([{ kind: 'overlap', withRowId: 'a' }])
    expect(analysis.ready).toBe(false)
  })
})

describe('fpsChangeImpact', () => {
  it('fps change keeps the hh mm ss ff labels and flags invalid frames', () => {
    const at30 = show(
      [
        row('late frame', { startTimecode: '00:10:00:28' }),
        row('fine', { startTimecode: '01:00:00:10' }),
        row('blank')
      ],
      30
    )
    expect(fpsChangeImpact(at30, 25)).toEqual({
      rowsWithStart: 2,
      invalidRowIds: ['late frame']
    })

    const at25: ShowData = { ...at30, fps: 25 }
    expect(at25.rows[0].startTimecode).toBe('00:10:00:28')
    expect(kindsOf(analyseShow(at25).byRow.get('late frame'))).toEqual(['invalidFrames'])

    const backAt30: ShowData = { ...at25, fps: 30 }
    expect(analyseShow(backAt30).byRow.get('late frame')).toBeUndefined()
  })
})
