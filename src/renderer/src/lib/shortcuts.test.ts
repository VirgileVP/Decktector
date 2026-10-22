import { describe, expect, it } from 'vitest'

import { shortcutFor, type ShortcutKeyEvent } from './shortcuts'

const press = (key: string, modifiers: Partial<ShortcutKeyEvent> = {}): ShortcutKeyEvent => ({
  key,
  ctrlKey: false,
  metaKey: false,
  altKey: false,
  shiftKey: false,
  repeat: false,
  ...modifiers
})

describe('shortcutFor', () => {
  it('toggles fullscreen on a bare F11', () => {
    expect(shortcutFor(press('F11'))).toBe('toggleFullscreen')
    expect(shortcutFor(press('F11', { ctrlKey: true }))).toBeNull()
  })

  it('opens the settings on Ctrl+, or Cmd+,', () => {
    expect(shortcutFor(press(',', { ctrlKey: true }))).toBe('openSettings')
    expect(shortcutFor(press(',', { metaKey: true }))).toBe('openSettings')
    expect(shortcutFor(press(','))).toBeNull()
    expect(shortcutFor(press(',', { ctrlKey: true, shiftKey: true }))).toBeNull()
  })

  it('ignores held-down keys and unrelated ones', () => {
    expect(shortcutFor(press('F11', { repeat: true }))).toBeNull()
    expect(shortcutFor(press('a', { ctrlKey: true }))).toBeNull()
  })
})
