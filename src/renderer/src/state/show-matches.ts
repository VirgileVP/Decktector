import { invoke, isTauri } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { create } from 'zustand'

import type { DeckId, DeckMatchStatus } from '../model'

interface DeckMatchesState {
  /** Where each deck that has reported stands against the show. */
  byDeck: Partial<Record<DeckId, DeckMatchStatus>>
}

export const useDeckMatches = create<DeckMatchesState>(() => ({ byDeck: {} }))

function store(statuses: readonly DeckMatchStatus[]): void {
  useDeckMatches.setState({
    byDeck: Object.fromEntries(statuses.map((status) => [status.deckId, status]))
  })
}

/**
 * Wires the store to the timecode service's deck matches and hydrates it
 * from the current ones. Returns the teardown function.
 *
 * Listeners are attached before the matches are requested, so nothing
 * emitted while we are hydrating is lost — same shape as
 * `connectTimecodeToEngine`.
 */
export function connectDeckMatchesToEngine(): () => void {
  if (!isTauri()) {
    return () => {}
  }

  const pending = listen<DeckMatchStatus[]>('timecode:deck-matches', ({ payload }) =>
    store(payload)
  )

  pending
    .then(() => invoke<DeckMatchStatus[]>('timecode_deck_matches'))
    .then(store)
    .catch((error) => console.error('Failed to hydrate the deck matches:', error))

  return () => {
    pending.then((unlisten) => unlisten(), ignoreAttachFailure)
  }
}

function ignoreAttachFailure(): void {}
