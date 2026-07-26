import type {
  DeckId,
  DeckState,
  DeviceInfo,
  EngineSnapshot,
  NetworkState,
  TrackInfo
} from '../engine/types'

/** Preferences only the main process can honour, since it owns the window. */
export interface WindowPrefs {
  alwaysOnTop: boolean
  /** Holds off the screen saver, for a set left running unattended. */
  preventSleep: boolean
}

/**
 * Surface exposed to the renderer as `window.decktective`.
 * Every `on*` subscription returns its unsubscribe function.
 */
export interface DecktectiveApi {
  getSnapshot(): Promise<EngineSnapshot>
  setWindowPrefs(prefs: WindowPrefs): Promise<void>
  onNetworkState(listener: (state: NetworkState) => void): () => void
  onDeviceConnected(listener: (device: DeviceInfo) => void): () => void
  onDeviceDisconnected(listener: (device: DeviceInfo) => void): () => void
  onDeckState(listener: (state: DeckState) => void): () => void
  onDeckTrack(listener: (deckId: DeckId, track: TrackInfo | null) => void): () => void
}
