/**
 * Picks which deck the timecode generator should read when it follows the
 * on-air deck instead of waiting for a manual pick in the header, and when
 * it hands over from one deck to the next.
 */

import type { DeckId, DeckState } from '../model'
import { isDeckActive, resolveOnAirSource, type OnAirSource } from './deck-activity'

/**
 * When following the on-air deck moves the source: once the old deck leaves
 * the air, or the moment a new deck goes on air.
 */
export const TIMECODE_HANDOVERS = ['oldLeaves', 'newOnAir'] as const
export type TimecodeHandover = (typeof TIMECODE_HANDOVERS)[number]

export const DEFAULT_TIMECODE_HANDOVER: TimecodeHandover = 'oldLeaves'

export const TIMECODE_HANDOVER_OPTIONS = [
  { value: 'oldLeaves', label: 'When the old deck leaves' },
  { value: 'newOnAir', label: 'When the new deck goes on air' }
] as const satisfies readonly { value: TimecodeHandover; label: string }[]

/** One line per choice, shown under the picker so it explains itself. */
export const TIMECODE_HANDOVER_DESCRIPTIONS: Record<TimecodeHandover, string> = {
  oldLeaves:
    'Timecode stays on the deck it reads until that deck leaves the air, then moves to the next one on air.',
  newOnAir:
    'Timecode moves to a deck the moment its channel goes on air, even while the old one is still up.'
}

/** Narrows an untrusted value (storage, another window) to a known handover; anything else is the default. */
export function parseTimecodeHandover(raw: unknown): TimecodeHandover {
  return TIMECODE_HANDOVERS.find((handover) => handover === raw) ?? DEFAULT_TIMECODE_HANDOVER
}

/**
 * The handover actually applied. `newOnAir` needs the mixer's on-air flag:
 * following playback, a deck cued in the headphones counts as on air and
 * would take the timecode, so it falls back to `oldLeaves`.
 */
export function effectiveHandover(
  setting: TimecodeHandover,
  onAirSource: OnAirSource,
  mixerCount: number
): TimecodeHandover {
  if (setting === 'newOnAir' && resolveOnAirSource(onAirSource, mixerCount) === 'mixer') {
    return 'newOnAir'
  }
  return 'oldLeaves'
}

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
 * With `newOnAir`, a deck that just went on air — in `onAirIds`, not in
 * `previousOnAirIds` — takes the source at once, the lowest number first when
 * several rise together. Otherwise, and always with `oldLeaves`: the current
 * source stays while it is still live — a crossfade where two decks are on
 * air at once must not flip the timecode back and forth — and when no deck
 * is live at all, so a gap between tracks does not cut the output; failing
 * that, the lowest-numbered live deck takes over.
 */
export function pickOnAirTimecodeSource(
  onAirIds: readonly DeckId[],
  previousOnAirIds: readonly DeckId[],
  currentSource: DeckId | null,
  handover: TimecodeHandover
): DeckId | null {
  if (handover === 'newOnAir') {
    const rising = onAirIds.filter((deckId) => !previousOnAirIds.includes(deckId))
    if (rising.length > 0) {
      const next = Math.min(...rising)
      return next === currentSource ? null : next
    }
  }
  if (onAirIds.length === 0) return null
  if (currentSource !== null && onAirIds.includes(currentSource)) return null
  return Math.min(...onAirIds)
}
