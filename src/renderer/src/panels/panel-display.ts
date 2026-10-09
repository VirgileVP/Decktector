/**
 * What a panel window shows — chosen per window from its own toolbar and
 * kept in that window's record (see `../state/panel-windows.ts`), so two
 * windows of the same deck can show different things and each one comes
 * back the same after a restart.
 *
 * Pure on purpose: the panel windows store reads this module, and the
 * component tree (which pops windows out) reads that store — importing any
 * component here would close the loop.
 */

import {
  DEFAULT_HEADER_BLOCK_DISPLAY,
  type HeaderBlockDisplay
} from '../components/HeaderBlockVisibility'
import { isRecord } from '../lib/is-record'
import { LINEUP_VIEW_OPTIONS, type DeckViewToggles, type LineupView } from '../state/settings'
import type { PanelId } from './params'

/** The display each panel owns — `null` for a panel with nothing to choose. */
export interface PanelDisplayByPanel {
  clock: null
  timecode: null
  'header-block': HeaderBlockDisplay
  /** Same toggles as the Deck view settings tab, owned by this window alone. */
  deck: DeckViewToggles
}

export type PanelDisplay = PanelDisplayByPanel[PanelId]

interface PanelDisplayCodec<T> {
  /** A brand-new window's display. `mainDeckView` is what the main window shows right now. */
  initial: (mainDeckView: DeckViewToggles) => T
  /** Re-validates a stored display field by field, taking anything missing or malformed from `fallback`. */
  parse: (raw: unknown, fallback: T) => T
}

/**
 * `fallback`, with every key `raw` holds a boolean for taken from `raw`.
 * Keys `fallback` does not have are dropped, which is also what lets a
 * toggle added in a later build show up with its default in a window
 * saved by an older one.
 */
export function parseBooleanFields<T extends Record<keyof T, boolean>>(
  raw: unknown,
  fallback: T
): T {
  if (!isRecord(raw)) return fallback
  const parsed = { ...fallback }
  for (const key of Object.keys(fallback) as (keyof T & string)[]) {
    const value = raw[key]
    if (typeof value === 'boolean') parsed[key] = value as T[keyof T & string]
  }
  return parsed
}

function parseLineupView(raw: unknown, fallback: LineupView): LineupView {
  const option = LINEUP_VIEW_OPTIONS.find((candidate) => candidate.value === raw)
  return option === undefined ? fallback : option.value
}

/** The Time window's show/hide toggles, plus its own choice of lineup readout. */
function parseHeaderBlockDisplay(raw: unknown, fallback: HeaderBlockDisplay): HeaderBlockDisplay {
  const { lineupView, ...visibility } = fallback
  return {
    ...parseBooleanFields(raw, visibility),
    lineupView: parseLineupView(isRecord(raw) ? raw.lineupView : undefined, lineupView)
  }
}

const NO_DISPLAY: PanelDisplayCodec<null> = {
  initial: () => null,
  parse: () => null
}

const PANEL_DISPLAY_CODECS: { [P in PanelId]: PanelDisplayCodec<PanelDisplayByPanel[P]> } = {
  clock: NO_DISPLAY,
  timecode: NO_DISPLAY,
  'header-block': {
    initial: () => ({ ...DEFAULT_HEADER_BLOCK_DISPLAY }),
    parse: parseHeaderBlockDisplay
  },
  deck: {
    // A copy, not a link: once open, the window is set from its own toolbar
    // and the Deck view settings tab only drives the main window.
    initial: (mainDeckView) => ({ ...mainDeckView }),
    parse: parseBooleanFields
  }
}

/** The display a freshly popped-out `panelId` window starts with. */
export function initialPanelDisplay<P extends PanelId>(
  panelId: P,
  mainDeckView: DeckViewToggles
): PanelDisplayByPanel[P] {
  return PANEL_DISPLAY_CODECS[panelId].initial(mainDeckView)
}

/**
 * A `panelId` window's display, read back from its stored record. Storage
 * is an outside boundary — hand-edited, or written by an older build — so
 * whatever does not validate falls back to what a new window would get.
 */
export function resolvePanelDisplay<P extends PanelId>(
  panelId: P,
  raw: unknown,
  mainDeckView: DeckViewToggles
): PanelDisplayByPanel[P] {
  const codec = PANEL_DISPLAY_CODECS[panelId]
  return codec.parse(raw, codec.initial(mainDeckView))
}
