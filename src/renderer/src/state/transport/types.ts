import type { DeckState, DeckTrack, DeviceInfo, EngineSnapshot, NetworkState } from '../../model'

/** What the store needs from whatever is feeding it engine events. */
export interface EngineHandlers {
  snapshot(snapshot: EngineSnapshot): void
  networkState(state: NetworkState): void
  deviceConnected(device: DeviceInfo): void
  deviceDisconnected(device: DeviceInfo): void
  deckState(deck: DeckState): void
  deckTrack(track: DeckTrack): void
}

/**
 * A source of engine events — the Tauri app shell for the desktop window,
 * a WebSocket for a remote browser. `connect` returns its teardown.
 */
export interface EngineTransport {
  connect(handlers: EngineHandlers): () => void
}
