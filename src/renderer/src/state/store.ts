import { create } from 'zustand'

import type {
  DeckId,
  DeckState,
  DeviceInfo,
  EngineSnapshot,
  NetworkState,
  TrackInfo
} from '../model'
import { selectTransport } from './transport'

interface DecktectorStore {
  networkState: NetworkState
  devices: DeviceInfo[]
  decks: Record<DeckId, DeckState>
  tracks: Record<DeckId, TrackInfo | null>
}

export const useDecktectorStore = create<DecktectorStore>(() => ({
  networkState: 'offline',
  devices: [],
  decks: {},
  tracks: {}
}))

/** Idempotent — a reconnecting transport re-hydrates by calling this again. */
function applySnapshot(snapshot: EngineSnapshot): void {
  useDecktectorStore.setState({
    networkState: snapshot.networkState,
    devices: snapshot.devices,
    decks: Object.fromEntries(snapshot.decks.map((deck) => [deck.deckId, deck])),
    tracks: Object.fromEntries(snapshot.tracks.map(({ deckId, track }) => [deckId, track]))
  })
}

/**
 * Wires the store to whichever transport is feeding this window, and
 * hydrates it from the current snapshot. Returns the teardown function.
 */
export function connectStoreToEngine(): () => void {
  const { setState } = useDecktectorStore

  return selectTransport().connect({
    snapshot: applySnapshot,
    networkState: (networkState) => setState({ networkState }),
    deviceConnected: (device) =>
      setState((s) => ({
        devices: [...s.devices.filter((d) => d.deckId !== device.deckId), device]
      })),
    deviceDisconnected: (device) =>
      setState((s) => {
        const decks = { ...s.decks }
        const tracks = { ...s.tracks }
        delete decks[device.deckId]
        delete tracks[device.deckId]
        return { devices: s.devices.filter((d) => d.deckId !== device.deckId), decks, tracks }
      }),
    deckState: (deck) => setState((s) => ({ decks: { ...s.decks, [deck.deckId]: deck } })),
    deckTrack: ({ deckId, track }) =>
      setState((s) => ({ tracks: { ...s.tracks, [deckId]: track } }))
  })
}
