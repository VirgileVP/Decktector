/**
 * Which show rows are playing right now: a row is playing when a deck runs
 * on it — matched to it by the timecode service — and that deck is playing.
 */

import type { DeckId, DeckMatchStatus, PlayState } from '../model'

/** Each playing row's id, with the decks playing it in deck order. */
export type PlayingRows = ReadonlyMap<string, readonly DeckId[]>

/**
 * The rows the decks are playing. `matches` says which row each deck runs
 * on; `playStates` says which decks are playing.
 */
export function playingRows(
  matches: Partial<Record<DeckId, DeckMatchStatus>>,
  playStates: Partial<Record<DeckId, PlayState>>
): PlayingRows {
  const rows = new Map<string, DeckId[]>()
  const statuses = Object.values(matches).filter((status) => status !== undefined)
  for (const { deckId, resolution } of statuses.sort((a, b) => a.deckId - b.deckId)) {
    if (resolution.type !== 'matched' || playStates[deckId] !== 'playing') continue
    rows.set(resolution.rowId, [...(rows.get(resolution.rowId) ?? []), deckId])
  }
  return rows
}
