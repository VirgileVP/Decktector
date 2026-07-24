import { create } from 'zustand'

import type {
  DeckId,
  DeckState,
  DeviceInfo,
  EngineSnapshot,
  NetworkState,
  TrackInfo
} from '../../../engine/types'

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
 * Wires the store to the engine IPC events and hydrates it from the
 * current snapshot. Returns the teardown function.
 */
export function connectStoreToEngine(): () => void {
  const { setState } = useDecktectiveStore
  const api = window.decktective
  if (!api) {
    // Renderer opened outside Electron (plain browser): no engine available
    console.warn('Decktective API unavailable; engine events will not be received')
    return () => {}
  }

  const unsubscribes = [
    api.onNetworkState((networkState) => setState({ networkState })),
    api.onDeviceConnected((device) =>
      setState((s) => ({
        devices: [...s.devices.filter((d) => d.deckId !== device.deckId), device]
      }))
    ),
    api.onDeviceDisconnected((device) =>
      setState((s) => {
        const decks = { ...s.decks }
        const tracks = { ...s.tracks }
        delete decks[device.deckId]
        delete tracks[device.deckId]
        return { devices: s.devices.filter((d) => d.deckId !== device.deckId), decks, tracks }
      })
    ),
    api.onDeckState((deck) => setState((s) => ({ decks: { ...s.decks, [deck.deckId]: deck } }))),
    api.onDeckTrack((deckId, track) =>
      setState((s) => ({ tracks: { ...s.tracks, [deckId]: track } }))
    )
  ]

  api.getSnapshot().then(applySnapshot, (error) => {
    console.error('Failed to load engine snapshot:', error)
  })

  return () => unsubscribes.forEach((unsubscribe) => unsubscribe())
}
