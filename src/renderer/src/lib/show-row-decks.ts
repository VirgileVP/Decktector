/**
 * Which decks hold each show row right now: a deck holds a row when the
 * timecode service matched its track to it, and it is playing that row or
 * only has it loaded.
 */

import type { DeckId, DeckMatchStatus, PlayState } from '../model'

/** The decks holding one row, in deck order. */
export interface RowDecks {
  playing: readonly DeckId[]
  /** Loaded on the deck — cued, paused, ended… — but not playing. */
  loaded: readonly DeckId[]
}

/** Each row a deck holds, by row id; rows no deck holds are absent. */
export type RowDecksById = ReadonlyMap<string, RowDecks>

/**
 * The rows the decks hold. `matches` says which row each deck runs on;
 * `playStates` says which of those decks are playing.
 */
export function rowDecks(
  matches: Partial<Record<DeckId, DeckMatchStatus>>,
  playStates: Partial<Record<DeckId, PlayState>>
): RowDecksById {
  const rows = new Map<string, { playing: DeckId[]; loaded: DeckId[] }>()
  const statuses = Object.values(matches).filter((status) => status !== undefined)
  for (const { deckId, resolution } of statuses.sort((a, b) => a.deckId - b.deckId)) {
    if (resolution.type !== 'matched') continue
    const decks = rows.get(resolution.rowId) ?? { playing: [], loaded: [] }
    if (playStates[deckId] === 'playing') decks.playing.push(deckId)
    else decks.loaded.push(deckId)
    rows.set(resolution.rowId, decks)
  }
  return rows
}
