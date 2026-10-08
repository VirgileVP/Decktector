import type { DeckViewToggles } from '../state/settings'

/** Which pieces of `DeckBar` are shown — one flag per `showDeck*` toggle. */
export interface DeckBarFields {
  artwork: boolean
  trackInfo: boolean
  trackTags: boolean
  tcButton: boolean
  transport: boolean
  syncBadges: boolean
  time: boolean
  timeRemaining: boolean
  pitch: boolean
  bpm: boolean
}

/**
 * Where the deck bar sits: `'bar'` across the top of the deck, `'rail'` down
 * its side between the waveforms and the beat indicator, or `'hidden'` when
 * every one of its toggles is off.
 */
export type DeckBarLayout = 'bar' | 'rail' | 'hidden'

/** The bar's slice of a set of Deck view toggles. */
export function deckBarFields(view: DeckViewToggles): DeckBarFields {
  return {
    artwork: view.showDeckArtwork,
    trackInfo: view.showDeckTrackInfo,
    trackTags: view.showDeckTrackTags,
    tcButton: view.showDeckTcButton,
    transport: view.showDeckTransport,
    syncBadges: view.showDeckSyncBadges,
    time: view.showDeckTime,
    timeRemaining: view.showDeckTimeRemaining,
    pitch: view.showDeckPitch,
    bpm: view.showDeckBpm
  }
}

/**
 * The identity block — artwork, title/artist and tags — is what
 * needs the deck's full width. With all three of those hidden, what's left is a
 * handful of narrow readouts that would leave a near-empty row above the
 * waveforms, so they stand up as a rail beside them instead.
 */
export function deckBarLayout(fields: DeckBarFields): DeckBarLayout {
  if (fields.artwork || fields.trackInfo || fields.trackTags) return 'bar'

  const hasReadouts =
    fields.tcButton ||
    fields.transport ||
    fields.syncBadges ||
    fields.time ||
    fields.pitch ||
    fields.bpm

  return hasReadouts ? 'rail' : 'hidden'
}
