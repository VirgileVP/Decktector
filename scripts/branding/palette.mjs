/**
 * Brand colors. The accent is ultraviolet, deliberately distinct from the app's state colors
 * (on-air red, ok green, warn amber, sync blue-gray) so the logo never reads as a state.
 */

const PALETTES = {
  dark: { bg: '#0b0c0f', ink: '#eef0f4', accent: '#8f6bff' },
  light: { bg: '#f5f5f2', ink: '#111318', accent: '#6a44f0' }
}

/** @param {'dark' | 'light'} mode */
export const colorsFor = (mode) => PALETTES[mode]

/** Background of the app icon tile: near-black with a hint of the accent. */
export const APP_ICON_TILE = '#16131f'
