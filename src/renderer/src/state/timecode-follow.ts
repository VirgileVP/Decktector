import { isTauri } from '@tauri-apps/api/core'

import { countMixers } from '../lib/deck-activity'
import { effectiveHandover, onAirDeckIds, pickOnAirTimecodeSource } from '../lib/timecode-follow'
import type { DeckId } from '../model'
import { useDevicesStore } from './devices'
import { useSettings } from './settings'
import { useDecktectorStore } from './store'
import { selectTimecodeSource, useTimecodeStore } from './timecode'

/**
 * Keeps the timecode source on the on-air deck while the `timecodeFollowOnAir`
 * setting is on, handing over as `timecodeHandover` says. Returns the
 * teardown function.
 *
 * Acts only when the set of live decks changes (or the setting is switched on),
 * not on every deck tick, so a manual pick in the header holds until the next
 * deck goes on air. The live decks seen last are kept to tell which deck just
 * went on air; the first look after switching on has nothing to compare
 * with, so no deck counts as having just gone on air.
 */
export function connectTimecodeFollowOnAir(): () => void {
  if (!isTauri()) {
    return () => {}
  }

  let previousOnAirIds: DeckId[] | null = null

  const sync = (): void => {
    const { timecodeFollowOnAir, onAirSource, timecodeHandover } = useSettings.getState()
    if (!timecodeFollowOnAir) {
      previousOnAirIds = null
      return
    }

    const mixerCount = countMixers(useDevicesStore.getState().devices)
    const onAirIds = onAirDeckIds(useDecktectorStore.getState().decks, onAirSource, mixerCount)
    const previous = previousOnAirIds
    if (previous !== null && onAirIds.join(',') === previous.join(',')) return
    previousOnAirIds = onAirIds

    const nextSource = pickOnAirTimecodeSource(
      onAirIds,
      previous ?? onAirIds,
      useTimecodeStore.getState().sourceDeckId,
      effectiveHandover(timecodeHandover, onAirSource, mixerCount)
    )
    if (nextSource !== null) selectTimecodeSource(nextSource)
  }

  sync()
  const unsubscribes = [
    useSettings.subscribe(sync),
    useDevicesStore.subscribe(sync),
    useDecktectorStore.subscribe(sync)
  ]
  return () => unsubscribes.forEach((unsubscribe) => unsubscribe())
}
