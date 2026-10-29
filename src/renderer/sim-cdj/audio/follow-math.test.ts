import { describe, expect, it } from 'vitest'

import type { SimDeckView } from '@renderer/model/SimDeckView'

import { driftMs, planPlayback, playbackRate, positionAt, wrapIntoLoop } from './follow-math'

const LOOP = { startMs: 1000, endMs: 3000 }

function deck(overrides: Partial<SimDeckView> = {}): SimDeckView {
  return {
    deckId: 1,
    trackIndex: 0,
    trackId: 7,
    playState: 'playing',
    positionMs: 5000,
    durationMs: 60_000,
    cuePointMs: 0,
    pitchFader: 0,
    tempoRange: 'six',
    effectivePitch: 0,
    trackBpm: 120,
    effectiveBpm: 120,
    beatInMeasure: 1,
    loopState: null,
    isMaster: false,
    isSynced: false,
    isOnAir: true,
    scratching: false,
    ...overrides
  }
}

describe('playbackRate', () => {
  it('follows the pitch percentage', () => {
    expect(playbackRate(0)).toBe(1)
    expect(playbackRate(8)).toBeCloseTo(1.08)
    expect(playbackRate(-100)).toBe(0)
  })
})

describe('wrapIntoLoop', () => {
  it('leaves positions before the loop end alone', () => {
    expect(wrapIntoLoop(2500, LOOP)).toBe(2500)
    expect(wrapIntoLoop(9000, null)).toBe(9000)
  })

  it('folds positions past the end back into the loop', () => {
    expect(wrapIntoLoop(3500, LOOP)).toBe(1500)
    expect(wrapIntoLoop(7500, LOOP)).toBe(1500)
  })
})

describe('positionAt', () => {
  it('advances at the anchored rate', () => {
    const anchor = { contextTimeSeconds: 10, positionMs: 4000, rate: 1.5 }
    expect(positionAt(anchor, 10.2, null)).toBeCloseTo(4300)
  })

  it('wraps inside a loop', () => {
    const anchor = { contextTimeSeconds: 0, positionMs: 2900, rate: 1 }
    expect(positionAt(anchor, 0.3, LOOP)).toBeCloseTo(1200)
  })
})

describe('driftMs', () => {
  it('is the plain difference outside a loop', () => {
    expect(driftMs(1000, 1060, null)).toBe(60)
    expect(driftMs(1000, 940, null)).toBe(-60)
  })

  it('takes the short way round the loop wrap point', () => {
    expect(driftMs(2990, 1010, LOOP)).toBeCloseTo(20)
    expect(driftMs(1010, 2990, LOOP)).toBeCloseTo(-20)
  })
})

describe('planPlayback', () => {
  it('extrapolates a playing deck over the age of the state', () => {
    const target = planPlayback(deck({ effectivePitch: 10 }), 1000, 1100)
    expect(target).toMatchObject({ kind: 'play', loop: null })
    expect(target.kind === 'play' && target.positionMs).toBeCloseTo(5110)
    expect(target.kind === 'play' && target.rate).toBeCloseTo(1.1)
  })

  it('never extrapolates backwards when clocks disagree', () => {
    expect(planPlayback(deck(), 1000, 900)).toMatchObject({ kind: 'play', positionMs: 5000 })
  })

  it('carries an active loop and wraps the extrapolated position into it', () => {
    const view = deck({ positionMs: 2990, loopState: { ...LOOP, active: true } })
    expect(planPlayback(view, 0, 100)).toMatchObject({ positionMs: 1090, loop: LOOP })
  })

  it('ignores a stored loop that is not running', () => {
    const view = deck({ loopState: { ...LOOP, active: false } })
    expect(planPlayback(view, 0, 0)).toMatchObject({ loop: null })
  })

  it.each(['paused', 'cued', 'ended', 'searching', 'empty', 'loading'] as const)(
    'is silent while %s',
    (playState) => {
      expect(planPlayback(deck({ playState }), 0, 0)).toEqual({ kind: 'silent' })
    }
  )

  it('is silent without a track', () => {
    expect(planPlayback(deck({ trackIndex: null }), 0, 0)).toEqual({ kind: 'silent' })
  })

  it('is silent when the tempo drops to a standstill', () => {
    expect(planPlayback(deck({ effectivePitch: -100 }), 0, 0)).toEqual({ kind: 'silent' })
  })

  it('switches to scratching, even from a pause', () => {
    expect(planPlayback(deck({ playState: 'paused', scratching: true }), 40, 90)).toEqual({
      kind: 'scratch',
      positionMs: 5000,
      atMs: 40
    })
  })
})
