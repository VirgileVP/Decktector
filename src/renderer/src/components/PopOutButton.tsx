import { isTauri } from '@tauri-apps/api/core'

import type { DeckId } from '../model'
import { openPanelWindow } from '../panels/open-panel-window'
import { isPanelWindow, type PanelId } from '../panels/params'

interface PopOutButtonProps {
  panelId: PanelId
  /** Only meaningful for `panelId === 'deck'`. */
  deckId: DeckId | null
  title: string
}

/**
 * Opens `panelId` (and, for a deck, `deckId`) in its own window. Renders
 * nothing in the remote browser view (no window API to call) or inside a
 * panel window itself (no popping a pop-out out of a pop-out).
 */
export function PopOutButton({ panelId, deckId, title }: PopOutButtonProps): React.JSX.Element | null {
  if (!isTauri() || isPanelWindow(window.location)) return null

  const handleClick = (): void => {
    // `instance` is a placeholder — `openPanelWindow` picks the real one.
    void openPanelWindow({ panelId, deckId, instance: 0 }).catch((error) =>
      console.error(`Failed to open the "${panelId}" panel window:`, error)
    )
  }

  return (
    <button
      type="button"
      className="pop-out-button"
      title={title}
      aria-label={title}
      onClick={handleClick}
    >
      <svg viewBox="0 0 14 14" width="14" height="14" aria-hidden="true">
        <path d="M6 2H2.6A.6.6 0 0 0 2 2.6v8.8c0 .33.27.6.6.6h8.8a.6.6 0 0 0 .6-.6V8" />
        <path d="M8.2 2H12v3.8M12 2 7.2 6.8" />
      </svg>
    </button>
  )
}
