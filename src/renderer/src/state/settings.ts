import { create } from 'zustand'
import { persist } from 'zustand/middleware'

import { DEFAULT_THEME, type ThemeId } from '../lib/theme'

/** Bounds of the sliders in the settings panel. */
export const WAVEFORM_DESATURATION_RANGE = { min: 0, max: 1, step: 0.05 } as const
export const UI_SCALE_RANGE = { min: 0.8, max: 1.6, step: 0.05 } as const

/** How much of the running order the header shows. */
export type LineupView = 'full' | 'around'

export const LINEUP_VIEW_OPTIONS = [
  { value: 'full', label: 'Whole lineup' },
  { value: 'around', label: 'Previous, current, next' }
] as const satisfies readonly { value: LineupView; label: string }[]

export interface Settings {
  theme: ThemeId
  /** 0 = raw rekordbox waveform colors, 1 = fully washed to the theme's target. */
  waveformDesaturation: number
  /** Fade decks that are not on air so the live ones read at a glance. */
  dimIdleDecks: boolean
  /** Renderer zoom factor, for booth screens from 13" to 27". */
  uiScale: number
  /** Whether the header shows the whole running order or only what is around now. */
  lineupView: LineupView
  /** Accent color overrides keyed by player number; absent means the default. */
  deckColors: Record<number, string>
  /** Keep the window above every other application. */
  alwaysOnTop: boolean
  /** Hold off the screen saver while a set is running. */
  preventSleep: boolean
}

export const DEFAULT_SETTINGS: Settings = {
  theme: DEFAULT_THEME,
  waveformDesaturation: 0.45,
  dimIdleDecks: true,
  uiScale: 1,
  lineupView: 'full',
  deckColors: {},
  alwaysOnTop: false,
  preventSleep: false
}

interface SettingsActions {
  update<K extends keyof Settings>(key: K, value: Settings[K]): void
  setDeckColor(deckId: number, color: string): void
  resetDeckColors(): void
  resetAll(): void
}

const SETTINGS_STORAGE_KEY = 'decktective.settings'

/**
 * User preferences, persisted as they change. Nothing here reaches the outside
 * world on its own — `connectSettingsToApp` is what applies them.
 */
export const useSettings = create<Settings & SettingsActions>()(
  persist(
    (set) => ({
      ...DEFAULT_SETTINGS,
      update: (key, value) => set({ [key]: value } as Partial<Settings>),
      setDeckColor: (deckId, color) =>
        set((state) => ({ deckColors: { ...state.deckColors, [deckId]: color } })),
      resetDeckColors: () => set({ deckColors: {} }),
      resetAll: () => set({ ...DEFAULT_SETTINGS })
    }),
    {
      name: SETTINGS_STORAGE_KEY,
      // Actions are not state: keeping them out avoids writing dead keys to storage.
      partialize: (state) => pickSettings(state)
    }
  )
)

function pickSettings(state: Settings): Settings {
  const {
    theme,
    waveformDesaturation,
    dimIdleDecks,
    uiScale,
    lineupView,
    deckColors,
    alwaysOnTop,
    preventSleep
  } = state
  return {
    theme,
    waveformDesaturation,
    dimIdleDecks,
    uiScale,
    lineupView,
    deckColors,
    alwaysOnTop,
    preventSleep
  }
}
