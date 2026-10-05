/**
 * Picks which deck the timecode generator should read when it follows the
 * on-air deck instead of waiting for a manual pick in the header.
 */

import type { DeckId, DeckState } from '../model'
import { isDeckActive, type OnAirSource } from './deck-activity'

/** Ids of the decks that currently count as live, ascending, per the on-air source setting. */
export function onAirDeckIds(
  decks: Readonly<Record<DeckId, DeckState>>,
  source: OnAirSource,
  mixerCount: number
): DeckId[] {
  return Object.values(decks)
    .filter((deck) => isDeckActive(source, deck, mixerCount))
    .map((deck) => deck.deckId)
    .sort((a, b) => a - b)
}

/**
 * The deck timecode should come from, or `null` to leave the source alone.
 *
 * The current source stays when it is still live — a crossfade where two decks
 * are on air at once must not flip the timecode back and forth — and when no
 * deck is live at all, so a gap between tracks does not cut the output.
 * Otherwise the lowest-numbered live deck takes over.
 */
export function pickOnAirTimecodeSource(
  onAirIds: readonly DeckId[],
  currentSource: DeckId | null
): DeckId | null {
  if (onAirIds.length === 0) return null
  if (currentSource !== null && onAirIds.includes(currentSource)) return null
  return onAirIds[0]
}
