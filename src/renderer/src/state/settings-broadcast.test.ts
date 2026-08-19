import { describe, expect, it } from 'vitest'
import { createApplyGuard, shouldApplyBroadcast, type SettingsBroadcast } from './settings-broadcast'
import type { Settings } from './settings'

// Only the `origin` field matters to the functions under test.
const SETTINGS_STUB = {} as Settings

function broadcastFrom(origin: string): SettingsBroadcast {
  return { origin, settings: SETTINGS_STUB }
}

describe('shouldApplyBroadcast', () => {
  it('is false for a broadcast originating from this window', () => {
    expect(shouldApplyBroadcast(broadcastFrom('main'), 'main')).toBe(false)
  })

  it('is true for a broadcast originating from another window', () => {
    expect(shouldApplyBroadcast(broadcastFrom('panel-clock-x-0'), 'main')).toBe(true)
  })
})

describe('createApplyGuard', () => {
  it('starts not applying', () => {
    expect(createApplyGuard().isApplying()).toBe(false)
  })

  it('is applying only for the duration of the wrapped call', () => {
    const guard = createApplyGuard()
    let observedDuring = false
    guard.wrapApply(() => {
      observedDuring = guard.isApplying()
    })
    expect(observedDuring).toBe(true)
    expect(guard.isApplying()).toBe(false)
  })

  it('clears the flag even when the wrapped call throws', () => {
    const guard = createApplyGuard()
    expect(() =>
      guard.wrapApply(() => {
        throw new Error('boom')
      })
    ).toThrow('boom')
    expect(guard.isApplying()).toBe(false)
  })
})
