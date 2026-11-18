import { useShow } from './show'
import { connectStoreBroadcast } from './store-broadcast'

/** Channel every window emits show changes on and listens to. */
export const SHOW_CHANNEL = 'show:changed'

/**
 * Keeps `useShow` the same in every window — see `connectStoreBroadcast`.
 * The main window edits the show; a Show panel window mirrors it, so its
 * table follows every edit, import and fill.
 */
export function connectShowBroadcast(): () => void {
  return connectStoreBroadcast(useShow, SHOW_CHANNEL, ({ show, name }) => ({ show, name }))
}
