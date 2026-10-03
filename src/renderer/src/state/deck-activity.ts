import type { DeckId } from '../model'
import {
  countMixers,
  hasAmbiguousMixers,
  isDeckActive,
  resolveOnAirSource,
  type EffectiveOnAirSource
} from '../lib/deck-activity'
import { useDevicesStore } from './devices'
import { useSettings } from './settings'
import { useDecktectiveStore } from './store'

/**
 * Distinct mixers on the Pro DJ Link network. The device list only exists in
 * the desktop shell (the remote viewer never receives it), so a browser always
 * sees zero.
 */
export function useMixerCount(): number {
  return useDevicesStore((s) => countMixers(s.devices))
}

/** The signal the UI follows once `auto` has been resolved. */
export function useEffectiveOnAirSource(): EffectiveOnAirSource {
  const source = useSettings((s) => s.onAirSource)
  const mixerCount = useMixerCount()
  return resolveOnAirSource(source, mixerCount)
}

/** Whether `auto` is following playback only because several mixers were detected. */
export function useAmbiguousMixers(): boolean {
  const source = useSettings((s) => s.onAirSource)
  const mixerCount = useMixerCount()
  return hasAmbiguousMixers(source, mixerCount)
}

/** Whether the deck counts as live, per the on-air source setting. */
export function useDeckActive(deckId: DeckId): boolean {
  const source = useSettings((s) => s.onAirSource)
  const mixerCount = useMixerCount()
  const deck = useDecktectiveStore((s) => s.decks[deckId])
  return isDeckActive(source, deck, mixerCount)
}
