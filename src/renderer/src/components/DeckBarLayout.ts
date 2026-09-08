import { useSettings } from '../state/settings'

/** Which pieces of `DeckBar` are shown — one flag per `showDeck*` setting. */
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

/** Reads the bar's slice of the settings store. */
export function useDeckBarFields(): DeckBarFields {
  return {
    artwork: useSettings((s) => s.showDeckArtwork),
    trackInfo: useSettings((s) => s.showDeckTrackInfo),
    trackTags: useSettings((s) => s.showDeckTrackTags),
    tcButton: useSettings((s) => s.showDeckTcButton),
    transport: useSettings((s) => s.showDeckTransport),
    syncBadges: useSettings((s) => s.showDeckSyncBadges),
    time: useSettings((s) => s.showDeckTime),
    timeRemaining: useSettings((s) => s.showDeckTimeRemaining),
    pitch: useSettings((s) => s.showDeckPitch),
    bpm: useSettings((s) => s.showDeckBpm)
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
