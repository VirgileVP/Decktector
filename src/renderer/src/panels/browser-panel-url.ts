/**
 * URL of a panel opened from the remote browser view. Kept apart from
 * `open-browser-panel-window.ts` so the rule stays testable without a DOM.
 */

import { remoteToken, TOKEN_PARAM } from '../lib/remote-url'
import { panelSearch, type PanelParams } from './params'

/**
 * Same page as `location`, showing `params` instead of the main view. The
 * remote access token is carried over — the new page reaches the engine
 * through the same gated server, so it needs the same credential.
 */
export function browserPanelUrl(location: Location, params: PanelParams): string {
  const search = new URLSearchParams(panelSearch(params))
  const token = remoteToken(location)
  if (token !== null) search.set(TOKEN_PARAM, token)
  return `${location.pathname}?${search.toString()}`
}
