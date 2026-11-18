import { describe, expect, it } from 'vitest'

import type { ShowData, ShowRow } from './show'
import {
  FILL_STEP_PRESETS,
  parseCustomStep,
  stepFrames,
  summariseIssues
} from './show-summary-view'
import { SHOW_FPS_OPTIONS } from './show-timecode'
import { analyseShow } from './show-validation'

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

describe('fill steps', () => {
  it('step presets convert to frames at every rate', () => {
    expect(FILL_STEP_PRESETS.map((preset) => preset.label)).toEqual(['10 min', '30 min', '1 h'])
    for (const fps of SHOW_FPS_OPTIONS) {
      expect(FILL_STEP_PRESETS.map((preset) => stepFrames(preset.seconds, fps))).toEqual([
        600 * fps,
        1800 * fps,
        3600 * fps
      ])
    }
  })

  it('a custom step of zero is refused', () => {
    expect(parseCustomStep('00:00:00')).toBeNull()
    expect(parseCustomStep('00:12:30')).toBe(750)
    expect(parseCustomStep(' 1:00:00 ')).toBe(3600)
    for (const text of ['', '12:30', '00:60:00', 'abc', '00:00:60']) {
      expect(parseCustomStep(text)).toBeNull()
    }
  })
})

describe('summariseIssues', () => {
  it('counts rows per kind, conflicts first, and names the first offender', () => {
    const show: ShowData = {
      fps: 25,
      fallbackStartTimecode: '23:00:00:00',
      cutWhenUnidentified: false,
      rows: [
        row('blank'),
        row('a', { startTimecode: '01:00:00:00' }),
        row('b', { startTimecode: '01:01:00:00' }),
        row('gone', { startTimecode: '05:00:00:00', absent: true })
      ]
    }
    expect(
      summariseIssues(show.rows, analyseShow(show)).map(({ kind, rows, firstRowId }) => [
        kind,
        rows,
        firstRowId
      ])
    ).toEqual([
      ['overlap', 2, 'a'],
      ['blankStart', 1, 'blank'],
      ['absent', 1, 'gone']
    ])
  })

  it('leaves duplicates sharing a start out of the conflicts', () => {
    const show: ShowData = {
      fps: 25,
      fallbackStartTimecode: '23:00:00:00',
      cutWhenUnidentified: false,
      rows: [
        row('a', { title: 'Same', startTimecode: '01:00:00:00' }),
        row('b', { title: 'Same', startTimecode: '01:00:00:00' })
      ]
    }
    expect(summariseIssues(show.rows, analyseShow(show))).toEqual([])
  })
})
