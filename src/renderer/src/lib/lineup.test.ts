import { afterEach, describe, expect, it } from 'vitest'

import { buildLineup, generateId, type LineupSlot } from './lineup'

const UUID_V4 = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/

/**
 * Restores whatever `crypto.randomUUID` was, so stubbing it out in one test
 * cannot leak into the next.
 */
const realRandomUUID = crypto.randomUUID

afterEach(() => {
  Object.defineProperty(crypto, 'randomUUID', {
    value: realRandomUUID,
    configurable: true
  })
})

describe('generateId', () => {
  it('produces a v4 UUID when crypto.randomUUID is available', () => {
    expect(generateId()).toMatch(UUID_V4)
  })

  /**
   * The remote view is served over plain HTTP from a LAN address, which is
   * not a secure context, so the browser does not expose
   * `crypto.randomUUID` there at all. Before the fallback existed, the
   * lineup's default slots threw on first render and the whole page stayed
   * blank.
   */
  it('still produces a v4 UUID when crypto.randomUUID is missing', () => {
    Object.defineProperty(crypto, 'randomUUID', { value: undefined, configurable: true })

    expect(generateId()).toMatch(UUID_V4)
  })

  it('does not repeat itself', () => {
    Object.defineProperty(crypto, 'randomUUID', { value: undefined, configurable: true })

    const ids = new Set(Array.from({ length: 500 }, () => generateId()))

    expect(ids.size).toBe(500)
  })
})

describe('buildLineup progress', () => {
  const MINUTES_PER_HOUR = 60
  const slots: LineupSlot[] = [
    { id: 'a', artist: 'Opener', note: '', startOffset: 22 * MINUTES_PER_HOUR },
    { id: 'b', artist: 'Closer', note: '', startOffset: 23 * MINUTES_PER_HOUR }
  ]
  const nightEnd = 24 * MINUTES_PER_HOUR

  function progressOf(nowClockMinutes: number, endOffset: number | null = nightEnd) {
    return buildLineup(slots, endOffset, nowClockMinutes).map((entry) => [
      entry.artist,
      entry.progress
    ])
  }

  it('measures how far the current set has run', () => {
    expect(progressOf(22 * MINUTES_PER_HOUR + 15)).toEqual([
      ['Opener', 0.25],
      ['Closer', null],
      ['END SHOW', null]
    ])
  })

  it('reads a clock finer than the minute', () => {
    const quarterMinute = 0.25
    const [opener] = buildLineup(slots, nightEnd, 22 * MINUTES_PER_HOUR + 6 + quarterMinute)

    expect(opener.progress).toBeCloseTo((6 + quarterMinute) / MINUTES_PER_HOUR)
  })

  it('starts the current set at zero', () => {
    expect(progressOf(23 * MINUTES_PER_HOUR)).toEqual([
      ['Opener', null],
      ['Closer', 0],
      ['END SHOW', null]
    ])
  })

  it('has no progress for a last set with no end to measure against', () => {
    expect(progressOf(23 * MINUTES_PER_HOUR + 30, null)).toEqual([
      ['Opener', null],
      ['Closer', null]
    ])
  })
})

describe('buildLineup notes', () => {
  const MINUTES_PER_HOUR = 60

  it("carries each set's note, and none on the doors and END SHOW markers", () => {
    const slots: LineupSlot[] = [
      { id: 'a', artist: 'Opener', note: 'Brings own mixer', startOffset: 22 * MINUTES_PER_HOUR },
      { id: 'b', artist: 'Closer', note: '', startOffset: 23 * MINUTES_PER_HOUR }
    ]
    const doors = 21 * MINUTES_PER_HOUR
    const nightEnd = 24 * MINUTES_PER_HOUR

    const notes = buildLineup(slots, nightEnd, 22 * MINUTES_PER_HOUR, doors).map((entry) => [
      entry.artist,
      entry.note
    ])

    expect(notes).toEqual([
      ['Doors opening', ''],
      ['Opener', 'Brings own mixer'],
      ['Closer', ''],
      ['END SHOW', '']
    ])
  })
})
