import { describe, expect, it } from 'vitest'
import { createStore } from 'zustand/vanilla'

import {
  bridgeStoreBroadcast,
  createApplyGuard,
  shouldApplyBroadcast,
  type BroadcastTransport,
  type StoreBroadcast
} from './store-broadcast'

// Only the `origin` field matters to the functions under test.
function broadcastFrom(origin: string): StoreBroadcast<null> {
  return { origin, state: null }
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

/** Past this many emits the fake bus stops delivering, so a bounce fails the test instead of hanging it. */
const MAX_DELIVERED_EMITS = 10

/**
 * An in-memory stand-in for Tauri's event bus: delivery and `listen` both
 * resolve asynchronously, as they do over IPC.
 */
function createFakeBus(): {
  transport: (origin: string) => BroadcastTransport
  emitted: () => number
} {
  const handlers = new Set<(payload: unknown) => void>()
  let emitted = 0

  return {
    emitted: () => emitted,
    transport: (origin) => ({
      origin,
      emit: async (_channel, payload) => {
        emitted++
        if (emitted > MAX_DELIVERED_EMITS) return
        await Promise.resolve()
        for (const handler of [...handlers]) handler(payload)
      },
      listen: async <T>(_channel: string, handler: (payload: T) => void) => {
        await Promise.resolve()
        const registered = handler as (payload: unknown) => void
        handlers.add(registered)
        return () => {
          handlers.delete(registered)
        }
      }
    })
  }
}

/** Enough turns of the microtask queue for any bounce between windows to show up. */
async function settle(): Promise<void> {
  for (let turn = 0; turn < 50; turn++) await Promise.resolve()
}

describe('bridgeStoreBroadcast', () => {
  /**
   * StrictMode mounts every effect twice, tearing the first one down before
   * `listen` has resolved. The first listener used to survive that teardown
   * and re-apply changes outside the live connection's guard, so each window
   * re-emitted what it received and the windows bounced every change between
   * them forever — the whole UI flickered.
   */
  it('emits a change once, even after a teardown that beat listen', async () => {
    const bus = createFakeBus()
    const main = createStore(() => ({ count: 0 }))
    const panel = createStore(() => ({ count: 0 }))
    const connect = (store: typeof main, origin: string): (() => void) =>
      bridgeStoreBroadcast(store, 'test', (state) => state, bus.transport(origin))

    connect(main, 'main')()
    connect(panel, 'panel')()
    connect(main, 'main')
    connect(panel, 'panel')
    await settle()

    main.setState({ count: 1 })
    await settle()

    expect(panel.getState().count).toBe(1)
    expect(bus.emitted()).toBe(1)
  })
})
