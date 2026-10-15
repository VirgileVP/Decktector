/**
 * Opens a panel in its own OS window. Every decision that shapes the window
 * — its label, its size, its URL — comes from an already-tested module;
 * this file only makes the Tauri call.
 */

import { getAllWebviewWindows, WebviewWindow } from '@tauri-apps/api/webviewWindow'

import { usePanelWindowsStore } from '../state/panel-windows'
import { pickDeckViewToggles, useSettings } from '../state/settings'
import { initialPanelDisplay } from './panel-display'
import { initialWindowBounds, type WindowGeometry } from './panel-geometry'
import { DEFAULT_PANEL_WINDOW_OPTIONS } from './panel-window-options'
import { panelSearch, type PanelParams } from './params'
import { PANELS } from './registry'
import { nextInstance, panelWindowLabel } from './window-label'

/**
 * The actual Tauri call, shared by `openPanelWindow` (which assigns a fresh
 * label) and `restorePanelWindows` (which reuses a label from the previous
 * session) — everything about how a panel window is created lives here
 * exactly once. `savedGeometry` (logical pixels, already fitted to the
 * connected monitors) places the window where it was last session.
 */
export function createPanelWebviewWindow(
  label: string,
  params: PanelParams,
  savedGeometry: WindowGeometry | null = null
): WebviewWindow {
  const definition = PANELS[params.panelId]

  const webview = new WebviewWindow(label, {
    url: `index.html${panelSearch(params)}`,
    title: definition.title(params),
    ...initialWindowBounds(definition.defaultSize, savedGeometry),
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

  return webview
}

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
  const label = panelWindowLabel(resolvedParams)

  createPanelWebviewWindow(label, resolvedParams)

  // The window itself reports its own geometry once mounted (`PanelChrome`);
  // this just makes it appear in the store immediately, instead of waiting
  // on that round trip.
  usePanelWindowsStore.getState().upsertWindow({
    label,
    params: resolvedParams,
    options: DEFAULT_PANEL_WINDOW_OPTIONS,
    display: initialPanelDisplay(params.panelId, pickDeckViewToggles(useSettings.getState())),
    geometry: null
  })
}
