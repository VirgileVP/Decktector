/**
 * URL contract for panel windows — the local, per-panel windows opened via
 * `openPanelWindow` (see `open-panel-window.ts`). Kept separate from
 * `../lib/remote-url.ts` on purpose: the remote server's `?deck=N` route must
 * not inherit these query params, and vice versa.
 */

import type { DeckId } from '../model'

/** Query string key naming which panel a window renders. */
const PANEL_PARAM = 'panel'

/** Query string key for the deck a `deck` panel is pinned to. */
const DECK_ID_PARAM = 'deckId'

/** Query string key distinguishing several windows of the same panel. */
const INSTANCE_PARAM = 'instance'

/** The fixed set of panels that can be popped into their own window. */
export type PanelId = 'clock' | 'timecode' | 'header-block' | 'deck'

/** Every panel id, for validating input and iterating the registry. */
export const PANEL_IDS: readonly PanelId[] = ['clock', 'timecode', 'header-block', 'deck']

export function isPanelId(value: string): value is PanelId {
  return (PANEL_IDS as readonly string[]).includes(value)
}

export interface PanelParams {
  panelId: PanelId
  /** Which deck the window shows. Only meaningful for `panelId === 'deck'`. */
  deckId: DeckId | null
  /** Distinguishes several windows of the same panel. */
  instance: number
}

/**
 * Parses a panel window's URL. Returns `null` when the URL does not describe
 * a panel window — in particular the main window's own URL, so callers can
 * route on this function alone.
 */
export function panelParams(location: Location): PanelParams | null {
  const search = new URLSearchParams(location.search)
  const rawPanelId = search.get(PANEL_PARAM)
  if (rawPanelId === null || !isPanelId(rawPanelId)) return null

  const deckId = parseDeckId(search.get(DECK_ID_PARAM))
  if (rawPanelId === 'deck' && deckId === null) return null

  return {
    panelId: rawPanelId,
    deckId: rawPanelId === 'deck' ? deckId : null,
    instance: parseInstance(search.get(INSTANCE_PARAM))
  }
}

function parseDeckId(raw: string | null): DeckId | null {
  if (raw === null) return null
  const deckId = Number(raw)
  return Number.isInteger(deckId) && deckId > 0 ? deckId : null
}

function parseInstance(raw: string | null): number {
  if (raw === null) return 0
  const instance = Number(raw)
  return Number.isInteger(instance) && instance >= 0 ? instance : 0
}

/** Builds the `?...` query string a panel window's URL carries. */
export function panelSearch(params: PanelParams): string {
  const search = new URLSearchParams()
  search.set(PANEL_PARAM, params.panelId)
  if (params.panelId === 'deck' && params.deckId !== null) {
    search.set(DECK_ID_PARAM, String(params.deckId))
  }
  search.set(INSTANCE_PARAM, String(params.instance))
  return `?${search.toString()}`
}

/** Whether `location` describes a panel window rather than the main window. */
export function isPanelWindow(location: Location): boolean {
  return panelParams(location) !== null
}
