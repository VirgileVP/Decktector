import { isTauri } from '@tauri-apps/api/core'

import { tauriTransport } from './tauri'
import type { EngineTransport } from './types'
import { websocketTransport } from './websocket'

export type { EngineHandlers, EngineTransport } from './types'

/**
 * The transport feeding this window: the desktop app shell when running
 * under Tauri, a WebSocket to the remote server otherwise (a browser tab
 * has no Tauri bridge to reach for).
 */
export function selectTransport(): EngineTransport {
  return isTauri() ? tauriTransport : websocketTransport
}
