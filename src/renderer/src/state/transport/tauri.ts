import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

import type { DeckState, DeckTrack, DeviceInfo, EngineSnapshot, NetworkState } from '../../model'
import type { EngineHandlers, EngineTransport } from './types'

/**
 * Feeds the store from the desktop app shell: engine events arrive as Tauri
 * events, and the current snapshot is fetched once through the
 * `engine_snapshot` command.
 *
 * Listeners are attached before the snapshot is requested, so nothing the
 * engine reports while we are hydrating is lost.
 */
export const tauriTransport: EngineTransport = {
  connect(handlers: EngineHandlers): () => void {
    const pending = subscribe(handlers)

    Promise.all(pending)
      .then(() => invoke<EngineSnapshot>('engine_snapshot'))
      .then((snapshot) => handlers.snapshot(snapshot))
      .catch((error) => console.error('Failed to hydrate from the engine snapshot:', error))

    return () => {
      for (const listener of pending) {
        // A listener still being attached when the store tears down must not
        // outlive it, so unsubscribe as soon as it resolves. Attach failures
        // were already reported by the hydration chain above.
        listener.then((unlisten) => unlisten(), ignoreAttachFailure)
      }
    }
  }
}

function subscribe(handlers: EngineHandlers): Promise<UnlistenFn>[] {
  return [
    listen<NetworkState>('engine:network:state', ({ payload }) => handlers.networkState(payload)),
    listen<DeviceInfo>('engine:device:connected', ({ payload }) => handlers.deviceConnected(payload)),
    listen<DeviceInfo>('engine:device:disconnected', ({ payload }) =>
      handlers.deviceDisconnected(payload)
    ),
    listen<DeckState>('engine:deck:state', ({ payload }) => handlers.deckState(payload)),
    listen<DeckTrack>('engine:deck:track', ({ payload }) => handlers.deckTrack(payload))
  ]
}

function ignoreAttachFailure(): void {}
