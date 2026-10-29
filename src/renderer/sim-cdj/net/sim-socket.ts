import type { SimCommand } from '@renderer/model/SimCommand'
import type { SimControlMessage } from '@renderer/model/SimControlMessage'

import { parseServerMessage } from './parse-message'

export type ConnectionStatus = 'connecting' | 'connected' | 'disconnected'

/** First wait before reconnecting; doubles after each failed attempt. */
export const RECONNECT_INITIAL_DELAY_MS = 500
export const RECONNECT_MAX_DELAY_MS = 5000
const RECONNECT_BACKOFF_FACTOR = 2

const SOCKET_PATH = '/ws'

/** The control server lives on the same host and port that served this page. */
export function controlSocketUrl(location: Pick<Location, 'protocol' | 'host'>): string {
  const scheme = location.protocol === 'https:' ? 'wss' : 'ws'
  return `${scheme}://${location.host}${SOCKET_PATH}`
}

/** Delay before the attempt following `failedAttempts` consecutive failures. */
export function reconnectDelayMs(failedAttempts: number): number {
  return Math.min(
    RECONNECT_MAX_DELAY_MS,
    RECONNECT_INITIAL_DELAY_MS * RECONNECT_BACKOFF_FACTOR ** failedAttempts
  )
}

export interface SimSocketHandlers {
  onMessage: (message: SimControlMessage) => void
  onStatus: (status: ConnectionStatus) => void
}

export interface SimSocket {
  /** Sends a command; returns false (and drops it) while the socket is not open. */
  send: (command: SimCommand) => boolean
  close: () => void
}

/** A WebSocket to the control server that reconnects on its own until closed. */
export function openSimSocket(url: string, handlers: SimSocketHandlers): SimSocket {
  let socket: WebSocket | null = null
  let retryTimer: number | null = null
  let failedAttempts = 0
  let closed = false

  const connect = (): void => {
    handlers.onStatus('connecting')
    const current = new WebSocket(url)
    socket = current

    current.onopen = () => {
      failedAttempts = 0
      handlers.onStatus('connected')
    }
    current.onmessage = (event: MessageEvent) => {
      if (typeof event.data !== 'string') {
        console.warn('Ignoring a non-text frame from the control server.')
        return
      }
      const message = parseServerMessage(event.data)
      if (message === null) {
        console.warn('Ignoring a malformed message from the control server.', event.data)
        return
      }
      handlers.onMessage(message)
    }
    current.onclose = () => {
      if (socket !== current) return
      socket = null
      handlers.onStatus('disconnected')
      if (closed) return
      retryTimer = window.setTimeout(connect, reconnectDelayMs(failedAttempts))
      failedAttempts += 1
    }
    current.onerror = () => {
      console.warn(`Control socket error talking to ${url}.`)
    }
  }

  connect()

  return {
    send: (command) => {
      if (socket?.readyState !== WebSocket.OPEN) return false
      socket.send(JSON.stringify(command))
      return true
    },
    close: () => {
      closed = true
      if (retryTimer !== null) window.clearTimeout(retryTimer)
      const current = socket
      socket = null
      current?.close()
    }
  }
}
