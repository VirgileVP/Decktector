import type {
  DeckId,
  DeckState,
  DeviceInfo,
  EngineSnapshot,
  NetworkState,
  TrackInfo
} from '../engine/types'

/**
 * Surface exposed to the renderer as `window.decktective`.
 * Every `on*` subscription returns its unsubscribe function.
 */
export interface DecktectiveApi {
  getSnapshot(): Promise<EngineSnapshot>
  onNetworkState(listener: (state: NetworkState) => void): () => void
  onDeviceConnected(listener: (device: DeviceInfo) => void): () => void
  onDeviceDisconnected(listener: (device: DeviceInfo) => void): () => void
  onDeckState(listener: (state: DeckState) => void): () => void
  onDeckTrack(listener: (deckId: DeckId, track: TrackInfo | null) => void): () => void
}
