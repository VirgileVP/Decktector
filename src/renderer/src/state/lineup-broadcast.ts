import { useLineup } from './lineup'
import { connectStoreBroadcast } from './store-broadcast'

/** Channel every window emits lineup changes on and listens to. */
export const LINEUP_CHANNEL = 'lineup:changed'

/**
 * Keeps `useLineup` the same in every window — see `connectStoreBroadcast`.
 * Without it the "Time" panel window would keep reading the lineup it was
 * opened with while the main window's settings edit it.
 */
export function connectLineupBroadcast(): () => void {
  return connectStoreBroadcast(useLineup, LINEUP_CHANNEL, ({ slots, endOffset, doorsOffset }) => ({
    slots,
    endOffset,
    doorsOffset
  }))
}
