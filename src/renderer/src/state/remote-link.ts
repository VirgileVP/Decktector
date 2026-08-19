import { create } from 'zustand'

interface RemoteLinkState {
  /** Whether the remote WebSocket is currently open. */
  connected: boolean
  /** Reconnect attempts since the last successful open, for a "reconnecting…" hint. */
  attempts: number
}

/**
 * Connection health of the remote WebSocket transport, so the UI can show a
 * banner while the link is down. The Tauri transport never touches this
 * store — it stays at its default, and the banner it feeds renders nothing.
 */
export const useRemoteLinkStore = create<RemoteLinkState>(() => ({
  connected: false,
  attempts: 0
}))
