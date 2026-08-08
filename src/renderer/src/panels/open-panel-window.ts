/**
 * Opens a panel in its own OS window. Every decision that shapes the window
 * — its label, its size, its URL — comes from an already-tested module;
 * this file only makes the Tauri call.
 */

import { getAllWebviewWindows, WebviewWindow } from '@tauri-apps/api/webviewWindow'

import { panelSearch, type PanelParams } from './params'
import { PANELS } from './registry'
import { nextInstance, panelWindowLabel } from './window-label'

/**
 * Opens `params.panelId` in a new window. The `instance` on `params` is
 * ignored in favour of the smallest free index among windows already open,
 * so callers (the pop-out buttons) never have to track instances themselves.
 */
export async function openPanelWindow(params: PanelParams): Promise<void> {
  const existingWindows = await getAllWebviewWindows()
  const instance = nextInstance(
    params.panelId,
    params.deckId,
    existingWindows.map((window) => window.label)
  )
  const resolvedParams: PanelParams = { ...params, instance }
  const definition = PANELS[resolvedParams.panelId]
  const label = panelWindowLabel(resolvedParams)

  const webview = new WebviewWindow(label, {
    url: `index.html${panelSearch(resolvedParams)}`,
    title: definition.title(resolvedParams),
    width: definition.defaultSize.width,
    height: definition.defaultSize.height,
    minWidth: definition.minSize.width,
    minHeight: definition.minSize.height,
    // Create-time only on Windows — see the "Gotchas" section of the panel
    // windows plan. Never attempt to flip this after creation.
    transparent: true,
    // Custom chrome (task 3.1) replaces the title bar and resize border this
    // would otherwise provide.
    decorations: false,
    shadow: false,
    // `backgroundColor` is deliberately omitted: setting one defeats
    // transparency.
    resizable: true,
    focus: true
  })

  void webview.once('tauri://error', (event) => {
    console.error(`Failed to open panel window "${label}":`, event)
  })
}
