import type { DeckId } from '../model'

/** Longest label that still fits a deck's badge without crowding the waveform. */
export const DECK_LABEL_MAX_LENGTH = 24

/** Operator-chosen names keyed by player number; a missing entry means "no label". */
export type DeckLabels = Record<DeckId, string>

/** Highest player number the Pro DJ Link network assigns. */
export const MAX_PLAYER_NUMBER = 6

/** Trims surrounding whitespace and caps the length. Non-strings count as empty. */
export function sanitizeDeckLabel(raw: unknown): string {
  return typeof raw === 'string' ? raw.trim().slice(0, DECK_LABEL_MAX_LENGTH).trimEnd() : ''
}

function isPlayerNumber(key: string): boolean {
  const deckId = Number(key)
  return Number.isInteger(deckId) && deckId >= 1 && deckId <= MAX_PLAYER_NUMBER
}

/**
 * Rebuilds a label map from untrusted input (persisted storage, another
 * window's broadcast). Unknown player numbers and empty labels are dropped.
 */
export function sanitizeDeckLabels(raw: unknown): DeckLabels {
  if (typeof raw !== 'object' || raw === null || Array.isArray(raw)) return {}
  const labels: DeckLabels = {}
  for (const [key, value] of Object.entries(raw)) {
    const label = sanitizeDeckLabel(value)
    if (isPlayerNumber(key) && label !== '') labels[Number(key)] = label
  }
  return labels
}

/** Copy of `labels` with one deck's label replaced; an empty label removes the entry. */
export function withDeckLabel(labels: DeckLabels, deckId: DeckId, raw: string): DeckLabels {
  const label = sanitizeDeckLabel(raw)
  const others = Object.fromEntries(
    Object.entries(labels).filter(([key]) => Number(key) !== deckId)
  ) as DeckLabels
  return label === '' ? others : { ...others, [deckId]: label }
}
