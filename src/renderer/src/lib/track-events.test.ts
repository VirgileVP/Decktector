import { describe, expect, it } from 'vitest'

import type { TrackEvent } from '../model'
import { eventName } from './track-events'

function event(overrides: Partial<TrackEvent>): TrackEvent {
  return { kind: 'groove', startMs: 0, endMs: 8000, confidence: 1, label: null, ...overrides }
}

describe('eventName', () => {
  it("uses rekordbox's own name for a phrase it analysed", () => {
    expect(eventName(event({ kind: 'groove', label: 'Verse 2' }))).toBe('Verse 2')
  })

  it('names an inferred section by its kind', () => {
    expect(eventName(event({ kind: 'drop', label: null }))).toBe('Drop')
  })
})
