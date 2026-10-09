import { useSettings } from './settings'
import { connectStoreBroadcast } from './store-broadcast'

/** Channel every window emits settings changes on and listens to. */
export const SETTINGS_CHANNEL = 'settings:changed'

/** Keeps `useSettings` the same in every window — see `connectStoreBroadcast`. */
export function connectSettingsBroadcast(): () => void {
  return connectStoreBroadcast(useSettings, SETTINGS_CHANNEL, (settings) => settings)
}
