import { useMemo } from 'react'
import { useShallow } from 'zustand/react/shallow'

import { rowDecks, type RowDecksById } from '../lib/show-row-decks'
import type { DeckId, PlayState } from '../model'
import { useDeckMatches } from './show-matches'
import { useDecktectorStore } from './store'

/**
 * The show rows the decks hold now, playing or only loaded. Subscribes to
 * each deck's play state only, so a playhead moving does not re-render the
 * show table.
 */
export function useRowDecks(): RowDecksById {
  const matches = useDeckMatches((state) => state.byDeck)
  const playStates = useDecktectorStore(
    useShallow((state) => {
      const states: Partial<Record<DeckId, PlayState>> = {}
      for (const deck of Object.values(state.decks)) states[deck.deckId] = deck.playState
      return states
    })
  )
  return useMemo(() => rowDecks(matches, playStates), [matches, playStates])
}
