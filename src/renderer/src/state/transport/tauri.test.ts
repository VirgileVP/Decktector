import { describe, expect, it, vi } from 'vitest'

import type { DeckTrack, EngineSnapshot } from '../../model'
import { createTauriTransport, RESYNC_CHANNEL, type TauriBridge } from './tauri'
import type { EngineHandlers } from './types'

type Listener = (event: { payload: unknown }) => void

/** An in-memory stand-in for the app shell: records listeners, serves snapshots in order. */
function fakeBridge(snapshots: EngineSnapshot[]): TauriBridge & {
  fire(channel: string, payload?: unknown): void
} {
  const listeners = new Map<string, Listener>()
  let served = 0
  return {
    listen(channel, handler) {
      listeners.set(channel, handler as Listener)
      return Promise.resolve(() => listeners.delete(channel))
    },
    fetchSnapshot() {
      return Promise.resolve(snapshots[Math.min(served++, snapshots.length - 1)])
    },
    fire(channel, payload = null) {
      listeners.get(channel)?.({ payload })
    }
  }
}

function recordingHandlers(): EngineHandlers & { snapshots: EngineSnapshot[] } {
  const snapshots: EngineSnapshot[] = []
  return {
    snapshots,
    snapshot: (snapshot) => snapshots.push(snapshot),
    networkState: vi.fn(),
    deviceConnected: vi.fn(),
    deviceDisconnected: vi.fn(),
    deckState: vi.fn(),
    deckTrack: vi.fn()
  }
}

function snapshotWithTrack(title: string): EngineSnapshot {
  const deckTrack = { deckId: 1, track: { title } } as unknown as DeckTrack
  return { networkState: 'connected', devices: [], decks: [], tracks: [deckTrack] }
}

/** Lets every already-settled promise callback run. */
const flush = (): Promise<void> => new Promise((resolve) => setTimeout(resolve, 0))

describe('createTauriTransport', () => {
  it('hydrates from the snapshot once listeners are attached', async () => {
    const before = snapshotWithTrack('Before')
    const handlers = recordingHandlers()

    createTauriTransport(fakeBridge([before])).connect(handlers)
    await flush()

    expect(handlers.snapshots).toEqual([before])
  })

  it('re-hydrates from a fresh snapshot when the shell reports dropped events', async () => {
    const before = snapshotWithTrack('Before')
    const after = snapshotWithTrack('After')
    const bridge = fakeBridge([before, after])
    const handlers = recordingHandlers()

    createTauriTransport(bridge).connect(handlers)
    await flush()
    bridge.fire(RESYNC_CHANNEL)
    await flush()

    expect(handlers.snapshots).toEqual([before, after])
  })

  it('stops resyncing once torn down', async () => {
    const bridge = fakeBridge([snapshotWithTrack('Before')])
    const handlers = recordingHandlers()

    const teardown = createTauriTransport(bridge).connect(handlers)
    await flush()
    teardown()
    await flush()
    bridge.fire(RESYNC_CHANNEL)
    await flush()

    expect(handlers.snapshots).toHaveLength(1)
  })
})
