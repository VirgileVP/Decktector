import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

import type { DeckState, DeckTrack, DeviceInfo, EngineSnapshot, NetworkState } from '../../model'
import type { EngineHandlers, EngineTransport } from './types'

/**
 * Channel the app shell emits on when its bridge fell behind the engine and
 * dropped events — mirrors `RESYNC_CHANNEL` in `src-tauri/src/events.rs`.
 */
export const RESYNC_CHANNEL = 'engine:resync'

/** The slice of the Tauri API this transport talks through. */
export interface TauriBridge {
  listen<T>(channel: string, handler: (event: { payload: T }) => void): Promise<UnlistenFn>
  fetchSnapshot(): Promise<EngineSnapshot>
}

/**
 * Feeds the store from the desktop app shell: engine events arrive as Tauri
 * events, and the current snapshot is fetched through the `engine_snapshot`
 * command — once on connect, and again whenever the shell reports it dropped
 * events, since a missed track change would otherwise stick until the next.
 *
 * Listeners are attached before the snapshot is requested, so nothing the
 * engine reports while we are hydrating is lost.
 */
export function createTauriTransport(bridge: TauriBridge): EngineTransport {
  return {
    connect(handlers: EngineHandlers): () => void {
      const hydrate = (): Promise<void> =>
        bridge
          .fetchSnapshot()
          .then((snapshot) => handlers.snapshot(snapshot))
          .catch((error) => console.error('Failed to hydrate from the engine snapshot:', error))

      const pending = [
        ...subscribe(bridge, handlers),
        bridge.listen<null>(RESYNC_CHANNEL, () => void hydrate())
      ]

      Promise.all(pending)
        .then(hydrate)
        .catch((error) => console.error('Failed to listen to the engine:', error))

      return () => {
        for (const listener of pending) {
          // A listener still being attached when the store tears down must
          // not outlive it, so unsubscribe as soon as it resolves. Attach
          // failures were already reported by the chain above.
          listener.then((unlisten) => unlisten(), ignoreAttachFailure)
        }
      }
    }
  }
}

export const tauriTransport: EngineTransport = createTauriTransport({
  listen,
  fetchSnapshot: () => invoke<EngineSnapshot>('engine_snapshot')
})

function subscribe(bridge: TauriBridge, handlers: EngineHandlers): Promise<UnlistenFn>[] {
  return [
    bridge.listen<NetworkState>('engine:network:state', ({ payload }) =>
      handlers.networkState(payload)
    ),
    bridge.listen<DeviceInfo>('engine:device:connected', ({ payload }) =>
      handlers.deviceConnected(payload)
    ),
    bridge.listen<DeviceInfo>('engine:device:disconnected', ({ payload }) =>
      handlers.deviceDisconnected(payload)
    ),
    bridge.listen<DeckState>('engine:deck:state', ({ payload }) => handlers.deckState(payload)),
    bridge.listen<DeckTrack>('engine:deck:track', ({ payload }) => handlers.deckTrack(payload))
  ]
}

function ignoreAttachFailure(): void {}
