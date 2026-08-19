import type { DeckState, DeckTrack, DeviceInfo, EngineSnapshot, NetworkState } from '../../model'
import { remoteSocketUrl } from '../../lib/remote-url'
import { useRemoteLinkStore } from '../remote-link'
import { rehydrateSnapshotTracks, rehydrateTrack } from './track-resources'
import type { EngineHandlers, EngineTransport } from './types'

/** First reconnect wait; doubles on every failed attempt up to the ceiling below. */
const INITIAL_RETRY_DELAY_MS = 250
/** Reconnect wait never grows past this, so a long outage still retries reasonably often. */
const MAX_RETRY_DELAY_MS = 5_000

/** Channel the server sends first on every connection, to hydrate the store. */
const SNAPSHOT_CHANNEL = 'engine:snapshot'

/** Envelope every message on the remote socket is wrapped in — mirrors `wire::RemoteMessage` on the Rust side. */
interface RemoteMessage {
  channel: string
  payload: unknown
}

const channelsWarnedAbout = new Set<string>()

/**
 * Feeds the store from the remote server: one WebSocket, reconnected with
 * exponential backoff, re-hydrated from a fresh snapshot on every open.
 * There is no client-side event replay — a dropped connection always
 * catches up through the snapshot the server sends first, never by trying
 * to remember what it missed.
 */
export const websocketTransport: EngineTransport = {
  connect(handlers: EngineHandlers): () => void {
    let socket: WebSocket | null = null
    let retryDelayMs = INITIAL_RETRY_DELAY_MS
    let retryTimer: ReturnType<typeof setTimeout> | null = null
    let tornDown = false

    const open = (): void => {
      const active = new WebSocket(remoteSocketUrl(window.location))
      socket = active

      active.addEventListener('open', () => {
        retryDelayMs = INITIAL_RETRY_DELAY_MS
        useRemoteLinkStore.setState({ connected: true, attempts: 0 })
      })

      active.addEventListener('message', (event: MessageEvent<string>) => {
        dispatch(handlers, event.data)
      })

      active.addEventListener('close', () => {
        useRemoteLinkStore.setState({ connected: false })
        if (tornDown) return
        useRemoteLinkStore.setState((state) => ({ attempts: state.attempts + 1 }))
        retryTimer = setTimeout(() => {
          retryDelayMs = Math.min(retryDelayMs * 2, MAX_RETRY_DELAY_MS)
          open()
        }, retryDelayMs)
      })

      // The 'close' event always follows 'error', so reconnection is
      // scheduled there — this only makes sure the socket doesn't linger.
      active.addEventListener('error', () => active.close())
    }

    open()

    return () => {
      tornDown = true
      if (retryTimer !== null) clearTimeout(retryTimer)
      socket?.close()
    }
  }
}

function dispatch(handlers: EngineHandlers, data: string): void {
  let message: RemoteMessage
  try {
    message = JSON.parse(data) as RemoteMessage
  } catch (error) {
    console.error('Failed to parse a remote message:', error)
    return
  }

  switch (message.channel) {
    case SNAPSHOT_CHANNEL: {
      const snapshot = message.payload as EngineSnapshot
      // Every loaded deck's artwork/waveform arrives stripped (see
      // `wire::from_snapshot`); this rehydrates them all before handing the
      // snapshot to the store, so it never sees a track missing either.
      void rehydrateSnapshotTracks(snapshot).then((tracks) =>
        handlers.snapshot({ ...snapshot, tracks })
      )
      return
    }
    case 'engine:network:state':
      handlers.networkState(message.payload as NetworkState)
      return
    case 'engine:device:connected':
      handlers.deviceConnected(message.payload as DeviceInfo)
      return
    case 'engine:device:disconnected':
      handlers.deviceDisconnected(message.payload as DeviceInfo)
      return
    case 'engine:deck:state':
      handlers.deckState(message.payload as DeckState)
      return
    case 'engine:deck:track': {
      const deckTrack = message.payload as DeckTrack
      // The artwork and waveform arrive stripped (see `wire::from_event`)
      // and are fetched separately, then merged back in before this reaches
      // the store — the store never has to know the wire shape differs.
      void rehydrateTrack(deckTrack.track).then((track) =>
        handlers.deckTrack({ ...deckTrack, track })
      )
      return
    }
    default:
      warnUnknownChannelOnce(message.channel)
  }
}

function warnUnknownChannelOnce(channel: string): void {
  if (channelsWarnedAbout.has(channel)) return
  channelsWarnedAbout.add(channel)
  console.warn('Unknown remote channel, ignoring:', channel)
}
