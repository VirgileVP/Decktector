/**
 * How the deck panels are laid out in the main window. The choice is a
 * per-device viewing preference, kept in `localStorage` on its own key rather
 * than in the settings store.
 */

export const DECK_VIEWS = ['stack', 'grid', 'quad'] as const
export type DeckView = (typeof DECK_VIEWS)[number]

export const DEFAULT_DECK_VIEW: DeckView = 'stack'

export const DECK_VIEW_STORAGE_KEY = 'decktective.view'

/** Narrows a stored value to a known view; anything else falls back to the default. */
export function parseDeckView(raw: string | null): DeckView {
  return DECK_VIEWS.find((view) => view === raw) ?? DEFAULT_DECK_VIEW
}

export function loadDeckView(): DeckView {
  return parseDeckView(localStorage.getItem(DECK_VIEW_STORAGE_KEY))
}

export function saveDeckView(view: DeckView): void {
  localStorage.setItem(DECK_VIEW_STORAGE_KEY, view)
}
