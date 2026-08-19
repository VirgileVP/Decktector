import { create } from 'zustand'
import { persist } from 'zustand/middleware'

import { DEFAULT_THEME, type ThemeId } from '../lib/theme'
import type { FrameRate } from '../model'

/** Bounds of the sliders in the settings panel. */
export const WAVEFORM_DESATURATION_RANGE = { min: 0, max: 1, step: 0.05 } as const
export const WAVEFORM_AMPLITUDE_RANGE = { min: 0.5, max: 2.5, step: 0.1 } as const
export const UI_SCALE_RANGE = { min: 0.8, max: 1.6, step: 0.05 } as const

/** Drag bounds for the cue list / track structure rails in the deck body. */
export const DECK_RAIL_WIDTH_RANGE = { min: 96, max: 360 } as const

/** How much of the running order the header shows. */
export type LineupView = 'full' | 'around'

export const LINEUP_VIEW_OPTIONS = [
  { value: 'full', label: 'Whole lineup' },
  { value: 'around', label: 'Previous, current, next' }
] as const satisfies readonly { value: LineupView; label: string }[]

/** How the detail waveform's columns are painted — mirrors the "WAVE COLOR"
 *  choice on real Pioneer hardware. `rgb` blends each column's frequency
 *  bands into one hue (the HD color waveform); `3band` keeps the bands
 *  separate as three stacked, semi-transparent bars from that same data;
 *  `blue` uses the classic pre-color mono waveform, falling back to a
 *  grayscale read of the color data when a track has no mono analysis. */
export type WaveformStyle = 'rgb' | '3band' | 'blue'

export const WAVEFORM_STYLE_OPTIONS = [
  { value: 'rgb', label: 'RGB' },
  { value: '3band', label: '3-band' },
  { value: 'blue', label: 'Blue' }
] as const satisfies readonly { value: WaveformStyle; label: string }[]

/** Where the timecode generator sends its output. `off` disables it entirely. */
export type TimecodeProtocolChoice = 'off' | 'artnet' | 'mtc' | 'ltc'

export const TIMECODE_PROTOCOL_OPTIONS = [
  { value: 'off', label: 'Off' },
  { value: 'artnet', label: 'Art-Net TC' },
  { value: 'mtc', label: 'MIDI Time Code' },
  { value: 'ltc', label: 'LTC (audio)' }
] as const satisfies readonly { value: TimecodeProtocolChoice; label: string }[]

export const TIMECODE_FRAME_RATE_OPTIONS = [
  { value: 'film24', label: '24 fps' },
  { value: 'ebu25', label: '25 fps' },
  { value: 'smpte30', label: '30 fps' }
] as const satisfies readonly { value: FrameRate; label: string }[]

/** Port every Art-Net node listens on by convention. */
export const ART_NET_DEFAULT_PORT = 6454

/** Port the remote view listens on unless the settings panel picks another — matches `DEFAULT_REMOTE_PORT` on the Rust side. */
export const REMOTE_DEFAULT_PORT = 7420

/** Placeholder interface name meaning "nothing picked yet". */
export const NO_NETWORK_INTERFACE = ''

/** Placeholder MIDI port name meaning "nothing picked yet". */
export const NO_MIDI_PORT = ''

/** Placeholder LTC output device name meaning "nothing picked yet". */
export const NO_LTC_DEVICE = ''

/** The Deck view tab's nine visibility toggles, grouped as one preset-able unit. */
export type DeckViewToggles = Pick<
  Settings,
  | 'showDeckArtwork'
  | 'showDeckTrackTags'
  | 'showDeckSyncBadges'
  | 'showDeckPitch'
  | 'showDeckTimeRemaining'
  | 'showDeckCueList'
  | 'showDeckEventList'
  | 'showDeckWaveformOverview'
  | 'showDeckWaveformDetail'
>

const DECK_VIEW_TOGGLE_KEYS = [
  'showDeckArtwork',
  'showDeckTrackTags',
  'showDeckSyncBadges',
  'showDeckPitch',
  'showDeckTimeRemaining',
  'showDeckCueList',
  'showDeckEventList',
  'showDeckWaveformOverview',
  'showDeckWaveformDetail'
] as const satisfies readonly (keyof DeckViewToggles)[]

export type DeckViewPreset = 'keying' | 'full'

/** Named starting points for the Deck view tab's nine independent toggles —
 *  still fully overridable below, this is a fast path, not a new mode. */
export const DECK_VIEW_PRESETS: Record<
  DeckViewPreset,
  { label: string; hint: string; values: DeckViewToggles }
> = {
  keying: {
    label: 'Keying',
    hint: 'Clean composite for a transparent panel window — art, on-air state, waveform shape',
    values: {
      showDeckArtwork: true,
      showDeckTrackTags: false,
      showDeckSyncBadges: true,
      showDeckPitch: false,
      showDeckTimeRemaining: true,
      showDeckCueList: false,
      showDeckEventList: false,
      showDeckWaveformOverview: true,
      showDeckWaveformDetail: false
    }
  },
  full: {
    label: 'Full',
    hint: 'Everything a booth screen shows',
    values: {
      showDeckArtwork: true,
      showDeckTrackTags: true,
      showDeckSyncBadges: true,
      showDeckPitch: true,
      showDeckTimeRemaining: true,
      showDeckCueList: true,
      showDeckEventList: true,
      showDeckWaveformOverview: true,
      showDeckWaveformDetail: true
    }
  }
}

/** Which preset (if any) the current Deck view toggles exactly match — `null`
 *  once the operator has hand-tweaked something, which is a valid state, not
 *  an error. */
export function matchingDeckViewPreset(settings: DeckViewToggles): DeckViewPreset | null {
  for (const [preset, { values }] of Object.entries(DECK_VIEW_PRESETS) as [
    DeckViewPreset,
    (typeof DECK_VIEW_PRESETS)[DeckViewPreset]
  ][]) {
    if (DECK_VIEW_TOGGLE_KEYS.every((key) => settings[key] === values[key])) return preset
  }
  return null
}

export interface Settings {
  theme: ThemeId
  /** 0 = raw rekordbox waveform colors, 1 = fully washed to the theme's target. */
  waveformDesaturation: number
  /** Gain multiplier on bar height, for tracks that read too quiet or too loud;
   *  clamped to the row at render time so it can never overflow. */
  waveformAmplitude: number
  /** How the detail waveform's columns are painted. */
  waveformStyle: WaveformStyle
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
  /** Protocol the timecode generator sends to; the deck it reads from is
   *  picked from the header, not here — see `state/timecode.ts`. */
  timecodeProtocol: TimecodeProtocolChoice
  timecodeFrameRate: FrameRate
  /** Name of the network adapter to broadcast from, e.g. `"eth0"` — picked
   *  from `list_network_interfaces`, not typed in. */
  timecodeArtNetInterface: string
  timecodeArtNetPort: number
  /** Name of the MIDI output port to send MTC to — picked from
   *  `list_midi_output_ports`, not typed in. */
  timecodeMtcPort: string
  /** Name of the audio output device to send LTC to — picked from
   *  `list_ltc_output_devices`, not typed in. */
  timecodeLtcDevice: string
  /** Whether the remote web view server is turned on. */
  remoteEnabled: boolean
  /** Port the remote server listens on. */
  remotePort: number
  /** Name of the network adapter the remote server binds to — picked from
   *  `list_network_interfaces`, not typed in. Empty means every interface. */
  remoteInterface: string
  /** Whether the deck bar shows the key/genre tags. */
  showDeckTrackTags: boolean
  /** Whether the deck bar shows the ON AIR / MASTER / SYNC badges. */
  showDeckSyncBadges: boolean
  /** Whether the time block shows the pitch percentage. */
  showDeckPitch: boolean
  /** Whether the time block shows the time remaining. */
  showDeckTimeRemaining: boolean
  /** Whether each deck shows its cue list. */
  showDeckCueList: boolean
  /** Whether each deck shows the analysed structure of its track. */
  showDeckEventList: boolean
  /** Whether the deck bar shows the track artwork. */
  showDeckArtwork: boolean
  /** Whether each deck shows the full-track waveform overview. */
  showDeckWaveformOverview: boolean
  /** Whether each deck shows the zoomed-in waveform detail. */
  showDeckWaveformDetail: boolean
  /** Width in px of the cue list rail, user-dragged via its resize handle. */
  deckCueListWidth: number
  /** Width in px of the track structure rail, user-dragged via its resize handle. */
  deckEventListWidth: number
}

export const DEFAULT_SETTINGS: Settings = {
  theme: DEFAULT_THEME,
  waveformDesaturation: 0.45,
  waveformAmplitude: 1,
  waveformStyle: 'rgb',
  dimIdleDecks: true,
  uiScale: 1,
  lineupView: 'full',
  deckColors: {},
  alwaysOnTop: false,
  preventSleep: false,
  timecodeProtocol: 'off',
  timecodeFrameRate: 'smpte30',
  timecodeArtNetInterface: NO_NETWORK_INTERFACE,
  timecodeArtNetPort: ART_NET_DEFAULT_PORT,
  timecodeMtcPort: NO_MIDI_PORT,
  timecodeLtcDevice: NO_LTC_DEVICE,
  remoteEnabled: false,
  remotePort: REMOTE_DEFAULT_PORT,
  remoteInterface: NO_NETWORK_INTERFACE,
  showDeckTrackTags: true,
  showDeckSyncBadges: true,
  showDeckPitch: true,
  showDeckTimeRemaining: true,
  showDeckCueList: true,
  showDeckEventList: true,
  showDeckArtwork: true,
  showDeckWaveformOverview: true,
  showDeckWaveformDetail: true,
  deckCueListWidth: 168,
  deckEventListWidth: 152
}

interface SettingsActions {
  update<K extends keyof Settings>(key: K, value: Settings[K]): void
  setDeckColor(deckId: number, color: string): void
  resetDeckColors(): void
  resetAll(): void
  applyDeckViewPreset(preset: DeckViewPreset): void
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
      resetAll: () => set({ ...DEFAULT_SETTINGS }),
      applyDeckViewPreset: (preset) => set({ ...DECK_VIEW_PRESETS[preset].values })
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
    waveformAmplitude,
    waveformStyle,
    dimIdleDecks,
    uiScale,
    lineupView,
    deckColors,
    alwaysOnTop,
    preventSleep,
    timecodeProtocol,
    timecodeFrameRate,
    timecodeArtNetInterface,
    timecodeArtNetPort,
    timecodeMtcPort,
    timecodeLtcDevice,
    remoteEnabled,
    remotePort,
    remoteInterface,
    showDeckTrackTags,
    showDeckSyncBadges,
    showDeckPitch,
    showDeckTimeRemaining,
    showDeckCueList,
    showDeckEventList,
    showDeckArtwork,
    showDeckWaveformOverview,
    showDeckWaveformDetail,
    deckCueListWidth,
    deckEventListWidth
  } = state
  return {
    theme,
    waveformDesaturation,
    waveformAmplitude,
    waveformStyle,
    dimIdleDecks,
    uiScale,
    lineupView,
    deckColors,
    alwaysOnTop,
    preventSleep,
    timecodeProtocol,
    timecodeFrameRate,
    timecodeArtNetInterface,
    timecodeArtNetPort,
    timecodeMtcPort,
    timecodeLtcDevice,
    remoteEnabled,
    remotePort,
    remoteInterface,
    showDeckTrackTags,
    showDeckSyncBadges,
    showDeckPitch,
    showDeckTimeRemaining,
    showDeckCueList,
    showDeckEventList,
    showDeckArtwork,
    showDeckWaveformOverview,
    showDeckWaveformDetail,
    deckCueListWidth,
    deckEventListWidth
  }
}
