/** Accent color per player number, applied to playheads and badges. */
export const DEFAULT_DECK_COLORS: Record<number, string> = {
  1: '#4da3ff',
  2: '#ffb84d',
  3: '#6ee7a0',
  4: '#ff6b81',
  5: '#c084fc',
  6: '#22d3ee'
}

/** Used for a player number outside the range Pro DJ Link allows. */
export const FALLBACK_DECK_COLOR = '#9aa4b2'

/** Pro DJ Link networks top out at 6 players. */
export const DECK_NUMBERS: readonly number[] = [1, 2, 3, 4, 5, 6]

export function deckColor(deckId: number, overrides: Record<number, string>): string {
  return overrides[deckId] ?? DEFAULT_DECK_COLORS[deckId] ?? FALLBACK_DECK_COLOR
}
