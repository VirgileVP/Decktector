import { create } from 'zustand'

import type { DeckId, TrackInfo } from '@renderer/model'
import type { SimDeckView } from '@renderer/model/SimDeckView'
import type { SimRigState } from '@renderer/model/SimRigState'

import type { ConnectionStatus } from '../net/sim-socket'

interface SimStore {
  status: ConnectionStatus
  rig: SimRigState | null
  tracks: Record<DeckId, TrackInfo | null>
  /** Where each deck's audio file is served from; null when the track has none. */
  audioUrls: Record<DeckId, string | null>
  /** Deck the track browser is open for, or null while it is closed. */
  browserDeck: DeckId | null
}

export const useSimStore = create<SimStore>(() => ({
  status: 'connecting',
  rig: null,
  tracks: {},
  audioUrls: {},
  browserDeck: null
}))

export function setConnectionStatus(status: ConnectionStatus): void {
  useSimStore.setState({ status })
}

export function setRigState(rig: SimRigState): void {
  useSimStore.setState({ rig })
}

export function setDeckTrack(
  deckId: DeckId,
  track: TrackInfo | null,
  audioUrl: string | null
): void {
  useSimStore.setState((state) => ({
    tracks: { ...state.tracks, [deckId]: track },
    audioUrls: { ...state.audioUrls, [deckId]: audioUrl }
  }))
}

export function openTrackBrowser(deckId: DeckId): void {
  useSimStore.setState({ browserDeck: deckId })
}

export function closeTrackBrowser(): void {
  useSimStore.setState({ browserDeck: null })
}

/** Live view of one deck, undefined until the first state message arrives. */
export function useDeckView(deckId: DeckId): SimDeckView | undefined {
  return useSimStore((state) => state.rig?.decks.find((deck) => deck.deckId === deckId))
}

export function useDeckTrack(deckId: DeckId): TrackInfo | null {
  return useSimStore((state) => state.tracks[deckId] ?? null)
}
