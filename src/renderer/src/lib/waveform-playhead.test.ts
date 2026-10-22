import { describe, expect, it } from 'vitest'

import { parseWaveformPlayheadPosition, playheadX } from './waveform-playhead'

describe('playheadX', () => {
  it('puts the centred playhead in the middle of the canvas', () => {
    expect(playheadX(1000, 'center')).toBe(500)
  })

  it('leaves most of the canvas for what is still to come when on the left', () => {
    const x = playheadX(1000, 'left')
    expect(x).toBeGreaterThan(0)
    expect(x).toBeLessThan(1000 / 2)
  })

  it('snaps to a whole CSS pixel', () => {
    expect(Number.isInteger(playheadX(333, 'center'))).toBe(true)
    expect(Number.isInteger(playheadX(333, 'left'))).toBe(true)
  })
})

describe('parseWaveformPlayheadPosition', () => {
  it('keeps a known position', () => {
    expect(parseWaveformPlayheadPosition('left')).toBe('left')
    expect(parseWaveformPlayheadPosition('center')).toBe('center')
  })

  it('falls back to center for anything else', () => {
    for (const raw of [undefined, null, 'right', 3, {}]) {
      expect(parseWaveformPlayheadPosition(raw)).toBe('center')
    }
  })
})
