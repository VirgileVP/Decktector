import { invoke, isTauri } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { create } from 'zustand'

import type {
  DeckId,
  DeckState,
  DeckTrack,
  DeviceInfo,
  EngineSnapshot,
  NetworkState,
  TrackInfo
} from '../model'

interface DecktectiveStore {
  networkState: NetworkState
  devices: DeviceInfo[]
  decks: Record<DeckId, DeckState>
  tracks: Record<DeckId, TrackInfo | null>
}

export const useDecktectiveStore = create<DecktectiveStore>(() => ({
  networkState: 'offline',
  devices: [],
  decks: {},
  tracks: {}
}))

function applySnapshot(snapshot: EngineSnapshot): void {
  useDecktectiveStore.setState({
    networkState: snapshot.networkState,
    devices: snapshot.devices,
    decks: Object.fromEntries(snapshot.decks.map((deck) => [deck.deckId, deck])),
    tracks: Object.fromEntries(snapshot.tracks.map(({ deckId, track }) => [deckId, track]))
  })
}

/**
 * Wires the store to the engine events and hydrates it from the current
 * snapshot. Returns the teardown function.
 *
 * Listeners are attached before the snapshot is requested, so nothing the
 * engine reports while we are hydrating is lost.
 */
export function connectStoreToEngine(): () => void {
  const { setState } = useDecktectiveStore

  if (!isTauri()) {
    // The renderer is being served on its own (a plain browser tab): the
    // Tauri bridge is absent and reaching for it throws synchronously, which
    // inside an effect would unmount the whole UI. No engine costs the deck
    // data, not the window.
    console.warn('Decktective app shell unavailable; engine events will not be received')
    return () => {}
  }

  const pending = subscribe(setState)

  Promise.all(pending)
    .then(() => invoke<EngineSnapshot>('engine_snapshot'))
    .then(applySnapshot)
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

type SetState = typeof useDecktectiveStore.setState

function subscribe(setState: SetState): Promise<UnlistenFn>[] {
  return [
    listen<NetworkState>('engine:network:state', ({ payload }) =>
      setState({ networkState: payload })
    ),
    listen<DeviceInfo>('engine:device:connected', ({ payload: device }) =>
      setState((s) => ({
        devices: [...s.devices.filter((d) => d.deckId !== device.deckId), device]
      }))
    ),
    listen<DeviceInfo>('engine:device:disconnected', ({ payload: device }) =>
      setState((s) => {
        const decks = { ...s.decks }
        const tracks = { ...s.tracks }
        delete decks[device.deckId]
        delete tracks[device.deckId]
        return { devices: s.devices.filter((d) => d.deckId !== device.deckId), decks, tracks }
      })
    ),
    listen<DeckState>('engine:deck:state', ({ payload: deck }) =>
      setState((s) => ({ decks: { ...s.decks, [deck.deckId]: deck } }))
    ),
    listen<DeckTrack>('engine:deck:track', ({ payload: { deckId, track } }) =>
      setState((s) => ({ tracks: { ...s.tracks, [deckId]: track } }))
    )
  ]
}

function ignoreAttachFailure(): void {}
