import { create } from 'zustand'
import { persist } from 'zustand/middleware'

import { DEFAULT_THEME, type ThemeId } from '../lib/theme'
import type { FrameRate } from '../model'

/** Bounds of the sliders in the settings panel. */
export const WAVEFORM_DESATURATION_RANGE = { min: 0, max: 1, step: 0.05 } as const
export const UI_SCALE_RANGE = { min: 0.8, max: 1.6, step: 0.05 } as const

/** How much of the running order the header shows. */
export type LineupView = 'full' | 'around'

export const LINEUP_VIEW_OPTIONS = [
  { value: 'full', label: 'Whole lineup' },
  { value: 'around', label: 'Previous, current, next' }
] as const satisfies readonly { value: LineupView; label: string }[]

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
  /** Whether the deck bar shows the track artwork. */
  showDeckArtwork: boolean
  /** Whether each deck shows the full-track waveform overview. */
  showDeckWaveformOverview: boolean
  /** Whether each deck shows the zoomed-in waveform detail. */
  showDeckWaveformDetail: boolean
}

export const DEFAULT_SETTINGS: Settings = {
  theme: DEFAULT_THEME,
  waveformDesaturation: 0.45,
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
  showDeckArtwork: true,
  showDeckWaveformOverview: true,
  showDeckWaveformDetail: true
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
    showDeckArtwork,
    showDeckWaveformOverview,
    showDeckWaveformDetail
  } = state
  return {
    theme,
    waveformDesaturation,
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
    showDeckArtwork,
    showDeckWaveformOverview,
    showDeckWaveformDetail
  }
}
