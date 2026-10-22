import { useEffect, useRef } from 'react'

import { shortcutFor, type AppShortcut } from '../lib/shortcuts'

/** What each shortcut does in a window; a shortcut left out does nothing there. */
export type AppShortcutHandlers = Partial<Record<AppShortcut, () => void>>

/** Listens for the app-wide keyboard shortcuts (see `lib/shortcuts.ts`) for as long as the window is mounted. */
export function useAppShortcuts(handlers: AppShortcutHandlers): void {
  // The listener is attached once; a ref keeps it calling the latest handlers.
  const latestHandlers = useRef(handlers)
  useEffect(() => {
    latestHandlers.current = handlers
  }, [handlers])

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent): void => {
      const shortcut = shortcutFor(event)
      const handler = shortcut ? latestHandlers.current[shortcut] : undefined
      if (!handler) return
      event.preventDefault()
      handler()
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [])
}
