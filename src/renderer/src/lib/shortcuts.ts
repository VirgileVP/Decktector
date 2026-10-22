/** App-wide keyboard shortcuts and the keys that trigger them. */

export type AppShortcut = 'toggleFullscreen' | 'openSettings'

/** The parts of a keyboard event a shortcut is matched on. */
export interface ShortcutKeyEvent {
  key: string
  ctrlKey: boolean
  metaKey: boolean
  altKey: boolean
  shiftKey: boolean
  repeat: boolean
}

/**
 * Which shortcut a key press asks for, if any: F11 toggles fullscreen,
 * Ctrl+, (Cmd+, on macOS) opens the settings. Held-down keys repeat no action.
 */
export function shortcutFor(event: ShortcutKeyEvent): AppShortcut | null {
  if (event.repeat) return null
  const noModifier = !event.ctrlKey && !event.metaKey && !event.altKey && !event.shiftKey
  if (event.key === 'F11' && noModifier) return 'toggleFullscreen'
  const commandOnly = (event.ctrlKey || event.metaKey) && !event.altKey && !event.shiftKey
  if (event.key === ',' && commandOnly) return 'openSettings'
  return null
}
