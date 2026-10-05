import { isTauri } from '@tauri-apps/api/core'

import { countMixers } from '../lib/deck-activity'
import { onAirDeckIds, pickOnAirTimecodeSource } from '../lib/timecode-follow'
import { useDevicesStore } from './devices'
import { useSettings } from './settings'
import { useDecktectorStore } from './store'
import { selectTimecodeSource, useTimecodeStore } from './timecode'

/**
 * Keeps the timecode source on the on-air deck while the `timecodeFollowOnAir`
 * setting is on. Returns the teardown function.
 *
 * Acts only when the set of live decks changes (or the setting is switched on),
 * not on every deck tick, so a manual pick in the header holds until the next
 * deck goes on air.
 */
export function connectTimecodeFollowOnAir(): () => void {
  if (!isTauri()) {
    return () => {}
  }

  let lastOnAirKey: string | null = null

  const sync = (): void => {
    const { timecodeFollowOnAir, onAirSource } = useSettings.getState()
    if (!timecodeFollowOnAir) {
      lastOnAirKey = null
      return
    }

    const mixerCount = countMixers(useDevicesStore.getState().devices)
    const onAirIds = onAirDeckIds(useDecktectorStore.getState().decks, onAirSource, mixerCount)
    const onAirKey = onAirIds.join(',')
    if (onAirKey === lastOnAirKey) return
    lastOnAirKey = onAirKey

    const nextSource = pickOnAirTimecodeSource(onAirIds, useTimecodeStore.getState().sourceDeckId)
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
