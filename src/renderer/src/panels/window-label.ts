/**
 * Tauri window labels for panel windows. Labels must be unique across the
 * app and are restricted to alphanumerics plus `-`, `_`, `/`, `:` — and the
 * capability file globs on the `panel-` prefix, so this format is part of
 * the contract, not an implementation detail.
 */

import type { DeckId } from '../model'
import { isPanelId, type PanelId, type PanelParams } from './params'

const LABEL_PREFIX = 'panel'

/** Placeholder segment for panels that are not pinned to a deck. */
const NO_DECK_SEGMENT = 'x'

/** Builds a panel window's label: `panel-<panelId>-<deckId | 'x'>-<instance>`. */
export function panelWindowLabel(params: PanelParams): string {
  const deckSegment = params.deckId === null ? NO_DECK_SEGMENT : String(params.deckId)
  return `${LABEL_PREFIX}-${params.panelId}-${deckSegment}-${params.instance}`
}

/**
 * The smallest instance index not already taken by a window of the same
 * panel (and, for `deck`, the same deck) — so closing window 0 and
 * reopening it reuses 0 rather than growing forever.
 */
export function nextInstance(
  panelId: PanelId,
  deckId: DeckId | null,
  taken: readonly string[]
): number {
  const takenInstances = new Set(
    taken
      .map(parsePanelLabel)
      .filter((params): params is PanelParams => params !== null)
      .filter((params) => params.panelId === panelId && params.deckId === deckId)
      .map((params) => params.instance)
  )

  let instance = 0
  while (takenInstances.has(instance)) instance++
  return instance
}

const LABEL_PATTERN = /^panel-([a-z-]+)-(\d+|x)-(\d+)$/

/**
 * The inverse of `panelWindowLabel`. Returns `null` for the main window's
 * label and for anything malformed — a caller restoring windows can skip
 * whatever it does not recognise instead of crashing on it.
 */
export function parsePanelLabel(label: string): PanelParams | null {
  const match = LABEL_PATTERN.exec(label)
  if (match === null) return null

  const [, panelId, deckSegment, instanceSegment] = match
  if (!isPanelId(panelId)) return null

  const deckId = deckSegment === NO_DECK_SEGMENT ? null : Number(deckSegment)
  if (panelId === 'deck' && (deckId === null || deckId <= 0)) return null

  return {
    panelId,
    deckId: panelId === 'deck' ? deckId : null,
    instance: Number(instanceSegment)
  }
}
