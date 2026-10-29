import type { DeckId, TrackInfo } from '@renderer/model'
import type { SimRigState } from '@renderer/model/SimRigState'
import { useDecktectorStore } from '@renderer/state/store'

import { toDeckState } from '../lib/deck-state-adapter'

/**
 * The reused waveform canvases read decks and tracks from the renderer's main
 * store (a separate instance in this page's module graph). These push the
 * simulator's state into it.
 */
export function publishDecks(rig: SimRigState): void {
  useDecktectorStore.setState({
    decks: Object.fromEntries(
      rig.decks.map((deck) => [deck.deckId, toDeckState(deck, rig.reportedAt)])
    )
  })
}

export function publishTrack(deckId: DeckId, track: TrackInfo | null): void {
  useDecktectorStore.setState((state) => ({ tracks: { ...state.tracks, [deckId]: track } }))
}
