/**
 * Which decks on air an edit of the show would move to another timecode —
 * so the editor can name them and ask before anything is pushed.
 *
 * A deck's start is its row's start timecode, or the show's fallback start
 * when it has no row (or its row has no start yet). Trims are not part of
 * it: a trim nudge applies at once, by decision, and never asks.
 */

import type { DeckId, DeckResolution } from '../model'
import type { ShowData } from './show'
import { identityMatches, type TrackIdentity } from './show-identity'

/** The base timecode with no show at all: every deck runs from midnight, as before shows. */
const NO_SHOW_START = '00:00:00:00'

/** A deck as the impact check sees it. */
export interface DeckForImpact {
  deckId: DeckId
  track: TrackIdentity | null
  onAir: boolean
}

/** Which row a deck was matched to by the timecode service, if any. */
export interface DeckRow {
  rowId: string | null
}

export interface DeckImpact {
  deckId: DeckId
  /** The start timecode label the deck runs from now. */
  before: string
  /** The one it would run from after the edit. */
  after: string
}

/**
 * The row a deck's resolution points at: its match, or the matched row
 * still waiting for a start timecode — typing that start moves the deck.
 */
export function rowOfResolution(resolution: DeckResolution): string | null {
  switch (resolution.type) {
    case 'matched':
      return resolution.rowId
    case 'fallback':
    case 'cut':
      return resolution.reason.type === 'blankStart' ? resolution.reason.rowId : null
    default:
      return null
  }
}

/**
 * The start `deck` runs from in `show`. A row id that is gone (a Replace
 * issues new ids) is matched again by identity, as the service does once.
 */
function startIn(show: ShowData | null, deck: DeckForImpact, rowId: string | null): string {
  if (show === null) return NO_SHOW_START
  if (rowId === null) return show.fallbackStartTimecode
  const kept = show.rows.find((row) => row.id === rowId)
  const track = deck.track
  const matching =
    kept !== undefined
      ? [kept]
      : track === null
        ? []
        : show.rows.filter((row) => identityMatches(row, track))
  const row = matching.length === 1 ? matching[0] : undefined
  return row?.startTimecode ?? show.fallbackStartTimecode
}

/** The decks on air whose start timecode differs between `before` and `after`. */
export function onAirImpact(
  before: ShowData | null,
  after: ShowData | null,
  decks: readonly DeckForImpact[],
  matches: Readonly<Partial<Record<DeckId, DeckRow>>>
): DeckImpact[] {
  return decks.flatMap((deck) => {
    if (!deck.onAir || deck.track === null) return []
    const rowId = matches[deck.deckId]?.rowId ?? null
    const impact = {
      deckId: deck.deckId,
      before: startIn(before, deck, rowId),
      after: startIn(after, deck, rowId)
    }
    return impact.before === impact.after ? [] : [impact]
  })
}
