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
  DEFAULT_HEADER_BLOCK_VISIBILITY,
  type HeaderBlockVisibility
} from '../components/HeaderBlockVisibility'
import { isRecord } from '../lib/is-record'
import type { DeckViewToggles } from '../state/settings'
import type { PanelId } from './params'

/** `'full'` is the main window's deck panel; `'compact'` is a single row sized for a small window. */
export type DeckWindowLayout = 'full' | 'compact'

/** Which pieces of the compact deck row are shown. */
export interface CompactDeckFields {
  artwork: boolean
  deckNumber: boolean
  /** The operator's deck label — shows nothing for a deck without one. */
  label: boolean
  title: boolean
  artist: boolean
  trackTags: boolean
  transport: boolean
  syncBadges: boolean
  time: boolean
  timeRemaining: boolean
  pitch: boolean
  bpm: boolean
}

/** What the compact row showed back when it could not be configured. */
export const DEFAULT_COMPACT_DECK_FIELDS: CompactDeckFields = {
  artwork: true,
  deckNumber: false,
  label: true,
  title: true,
  artist: true,
  trackTags: false,
  transport: true,
  syncBadges: false,
  time: false,
  timeRemaining: true,
  pitch: false,
  bpm: true
}

/**
 * A deck window's display. Each layout keeps its own set of toggles, so
 * switching layout and back loses nothing.
 */
export interface DeckWindowDisplay {
  layout: DeckWindowLayout
  /** Same toggles as the Deck view settings tab, owned by this window alone. */
  full: DeckViewToggles
  compact: CompactDeckFields
}

/** The display each panel owns — `null` for a panel with nothing to choose. */
export interface PanelDisplayByPanel {
  clock: null
  timecode: null
  'header-block': HeaderBlockVisibility
  deck: DeckWindowDisplay
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

function parseDeckWindowLayout(raw: unknown, fallback: DeckWindowLayout): DeckWindowLayout {
  return raw === 'full' || raw === 'compact' ? raw : fallback
}

function parseDeckWindowDisplay(raw: unknown, fallback: DeckWindowDisplay): DeckWindowDisplay {
  if (!isRecord(raw)) return fallback
  return {
    layout: parseDeckWindowLayout(raw.layout, fallback.layout),
    full: parseBooleanFields(raw.full, fallback.full),
    compact: parseBooleanFields(raw.compact, fallback.compact)
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
    initial: () => ({ ...DEFAULT_HEADER_BLOCK_VISIBILITY }),
    parse: parseBooleanFields
  },
  deck: {
    // A copy, not a link: once open, the window is set from its own toolbar
    // and the Deck view settings tab only drives the main window.
    initial: (mainDeckView) => ({
      layout: 'full',
      full: { ...mainDeckView },
      compact: { ...DEFAULT_COMPACT_DECK_FIELDS }
    }),
    parse: parseDeckWindowDisplay
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
