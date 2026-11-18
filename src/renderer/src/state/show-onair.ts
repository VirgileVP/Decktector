import { countMixers, isDeckActive } from '../lib/deck-activity'
import type { DeckForImpact, DeckRow } from '../lib/show-onair-impact'
import { rowOfResolution } from '../lib/show-onair-impact'
import type { DeckId } from '../model'
import { useDevicesStore } from './devices'
import { useSettings } from './settings'
import { useDeckMatches } from './show-matches'
import { useDecktectorStore } from './store'

const MS_PER_SECOND = 1000

/**
 * Every deck as the on-air impact check needs it, read from the stores right
 * now: what it holds, and whether it counts as on air under the on-air
 * source setting — the same rule the deck bars follow.
 */
export function currentDecksForImpact(): DeckForImpact[] {
  const { decks, tracks } = useDecktectorStore.getState()
  const source = useSettings.getState().onAirSource
  const mixerCount = countMixers(useDevicesStore.getState().devices)
  return Object.values(decks).map((deck) => {
    const track = tracks[deck.deckId] ?? null
    return {
      deckId: deck.deckId,
      onAir: isDeckActive(source, deck, mixerCount),
      track:
        track === null
          ? null
          : {
              title: track.title,
              artist: track.artist ?? '',
              durationSeconds: Math.round(track.durationMs / MS_PER_SECOND)
            }
    }
  })
}

/** The row each deck runs on, as the timecode service last reported. */
export function currentDeckRows(): Partial<Record<DeckId, DeckRow>> {
  const { byDeck } = useDeckMatches.getState()
  return Object.fromEntries(
    Object.values(byDeck).flatMap((status) =>
      status === undefined ? [] : [[status.deckId, { rowId: rowOfResolution(status.resolution) }]]
    )
  )
}
