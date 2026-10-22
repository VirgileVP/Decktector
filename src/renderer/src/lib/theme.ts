/**
 * Theme catalogue. Palettes live in `assets/themes.css`, keyed by the same ids
 * as `THEMES` below; the active choice is held by the settings store.
 */

export const THEME_IDS = [
  'original',
  'dark',
  'light',
  'contrast',
  'amber',
  'midnight',
  'monochrome'
] as const
export type ThemeId = (typeof THEME_IDS)[number]

export interface ThemeOption {
  id: ThemeId
  label: string
  description: string
}

export const THEMES: readonly ThemeOption[] = [
  { id: 'original', label: 'Original', description: 'Near-black with cyan accents' },
  { id: 'dark', label: 'Dark', description: 'Neutral greys, softer than Original' },
  { id: 'light', label: 'Light', description: 'For bright rooms and daylight setups' },
  { id: 'contrast', label: 'High contrast', description: 'Pure black and saturated accents' },
  { id: 'amber', label: 'Amber CRT', description: 'Monochrome amber, vintage hardware' },
  { id: 'midnight', label: 'Midnight', description: 'Deep blue, easy on long sessions' },
  { id: 'monochrome', label: 'Monochrome', description: 'Pure greyscale, no hue anywhere' }
] as const

export const DEFAULT_THEME: ThemeId = 'original'

/** Narrows an untrusted value (storage, another window) to a known theme; anything else is the default. */
export function parseTheme(raw: unknown): ThemeId {
  return THEME_IDS.find((theme) => theme === raw) ?? DEFAULT_THEME
}

/** Swaps the palette the whole document reads its colors from. */
export function applyTheme(theme: ThemeId): void {
  document.documentElement.dataset.theme = theme
}
