/**
 * Opens a panel in its own browser window, for the remote view — the
 * counterpart of `open-panel-window.ts`, which needs the Tauri window API.
 * The browser gives no control over transparency, always-on-top or saved
 * placement, so the window is a plain popup sized like a fresh desktop one.
 */

import { browserPanelUrl } from './browser-panel-url'
import type { PanelParams } from './params'
import { PANELS } from './registry'

export function openBrowserPanelWindow(params: PanelParams): void {
  const { width, height } = PANELS[params.panelId].defaultSize
  const opened = window.open(
    browserPanelUrl(window.location, params),
    '_blank',
    `popup,width=${width},height=${height}`
  )
  if (opened === null) {
    throw new Error('The browser blocked the popup window.')
  }
}
