import { useMemo } from 'react'
import { useShallow } from 'zustand/react/shallow'

import { playingRows, type PlayingRows } from '../lib/show-playing'
import type { DeckId, PlayState } from '../model'
import { useDeckMatches } from './show-matches'
import { useDecktectorStore } from './store'

/**
 * The show rows the decks are playing now. Subscribes to each deck's play
 * state only, so a playhead moving does not re-render the show table.
 */
export function usePlayingRows(): PlayingRows {
  const matches = useDeckMatches((state) => state.byDeck)
  const playStates = useDecktectorStore(
    useShallow((state) => {
      const states: Partial<Record<DeckId, PlayState>> = {}
      for (const deck of Object.values(state.decks)) states[deck.deckId] = deck.playState
      return states
    })
  )
  return useMemo(() => playingRows(matches, playStates), [matches, playStates])
}
