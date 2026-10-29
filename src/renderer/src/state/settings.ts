import { create } from 'zustand'
import { persist } from 'zustand/middleware'
import { useShallow } from 'zustand/react/shallow'

import { DEFAULT_ON_AIR_SOURCE, parseOnAirSource, type OnAirSource } from '../lib/deck-activity'
import { sanitizeDeckLabels, type DeckLabels } from '../lib/deck-labels'
import { DEFAULT_THEME, parseTheme, type ThemeId } from '../lib/theme'
import {
  DEFAULT_WAVEFORM_PLAYHEAD_POSITION,
  parseWaveformPlayheadPosition,
  type WaveformPlayheadPosition
} from '../lib/waveform-playhead'
import type { FrameRate } from '../model'

/** Bounds of the sliders in the settings panel. */
export const UI_SCALE_RANGE = { min: 0.8, max: 1.6, step: 0.05 } as const
export const WAVEFORM_SATURATION_RANGE = { min: 0, max: 1, step: 0.05 } as const

/** Drag bounds for the cue list / track structure rails in the deck body. */
export const DECK_RAIL_WIDTH_RANGE = { min: 96, max: 360 } as const

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

/** Default maximum concurrent clients for the remote server. */
export const REMOTE_DEFAULT_MAX_CLIENTS = 4

/** Bounds of the client count slider in the remote view settings. */
export const REMOTE_MAX_CLIENTS_RANGE = { min: 1, max: 16, step: 1 } as const

/** Placeholder interface name meaning "nothing picked yet". */
export const NO_NETWORK_INTERFACE = ''

/** Placeholder MIDI port name meaning "nothing picked yet". */
export const NO_MIDI_PORT = ''

/** Placeholder LTC output device name meaning "nothing picked yet". */
export const NO_LTC_DEVICE = ''

/** The Deck view tab's fifteen visibility toggles, grouped as one preset-able unit. */
export type DeckViewToggles = Pick<
  Settings,
  | 'showDeckNumber'
  | 'showDeckArtwork'
  | 'showDeckTrackInfo'
  | 'showDeckTrackTags'
  | 'showDeckTcButton'
  | 'showDeckTransport'
  | 'showDeckSyncBadges'
  | 'showDeckTime'
  | 'showDeckPitch'
  | 'showDeckTimeRemaining'
  | 'showDeckBpm'
  | 'showDeckCueList'
  | 'showDeckEventList'
  | 'showDeckWaveformOverview'
  | 'showDeckWaveformDetail'
>

export const DECK_VIEW_TOGGLE_KEYS = [
  'showDeckNumber',
  'showDeckArtwork',
  'showDeckTrackInfo',
  'showDeckTrackTags',
  'showDeckTcButton',
  'showDeckTransport',
  'showDeckSyncBadges',
  'showDeckTime',
  'showDeckPitch',
  'showDeckTimeRemaining',
  'showDeckBpm',
  'showDeckCueList',
  'showDeckEventList',
  'showDeckWaveformOverview',
  'showDeckWaveformDetail'
] as const satisfies readonly (keyof DeckViewToggles)[]

/** The Deck view toggles out of a full settings object — a copy, safe to keep. */
export function pickDeckViewToggles(settings: DeckViewToggles): DeckViewToggles {
  const toggles = {} as DeckViewToggles
  for (const key of DECK_VIEW_TOGGLE_KEYS) toggles[key] = settings[key]
  return toggles
}

export type DeckViewPreset = 'simple' | 'full'

/** Deck view toggles every new install starts with: everything but the track
 *  structure, which is analysis-heavy and rarely wanted on a booth screen. */
const FULL_DECK_VIEW: DeckViewToggles = {
  showDeckNumber: true,
  showDeckArtwork: true,
  showDeckTrackInfo: true,
  showDeckTrackTags: true,
  showDeckTcButton: true,
  showDeckTransport: true,
  showDeckSyncBadges: true,
  showDeckTime: true,
  showDeckPitch: true,
  showDeckTimeRemaining: true,
  showDeckBpm: true,
  showDeckCueList: true,
  showDeckEventList: false,
  showDeckWaveformOverview: true,
  showDeckWaveformDetail: true
}

/** Named starting points for the Deck view tab's fifteen independent toggles —
 *  still fully overridable below, this is a fast path, not a new mode. */
export const DECK_VIEW_PRESETS: Record<
  DeckViewPreset,
  { label: string; hint: string; values: DeckViewToggles }
> = {
  simple: {
    label: 'Simple view',
    hint: 'Timecode, transport, sync badges, timing, cue list and the detail waveform',
    values: {
      showDeckNumber: true,
      showDeckArtwork: false,
      showDeckTrackInfo: false,
      showDeckTrackTags: false,
      showDeckTcButton: true,
      showDeckTransport: true,
      showDeckSyncBadges: true,
      showDeckTime: true,
      showDeckPitch: false,
      showDeckTimeRemaining: true,
      showDeckBpm: true,
      showDeckCueList: true,
      showDeckEventList: false,
      showDeckWaveformOverview: false,
      showDeckWaveformDetail: true
    }
  },
  full: {
    label: 'Full',
    hint: 'Everything except the track structure',
    values: FULL_DECK_VIEW
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
  /** 1 = raw rekordbox waveform colors, 0 = fully white — a CDJ-style
   *  crossfade between the "Color" and "White" waveform display modes. */
  waveformSaturation: number
  /** Whether waveforms are rendered symmetrically around the center line (CDJ-style) or bottom-aligned. */
  waveformMirrored: boolean
  /** Blurs the detail waveform slightly so neighbouring rekordbox colors blend
   *  into a gradient instead of a hard-edged patchwork of solid columns. */
  waveformSmoothing: boolean
  /** Where the detail waveform's playhead sits — centred, or on the left so
   *  most of the width shows what is coming up. */
  waveformPlayheadPosition: WaveformPlayheadPosition
  /** Fade decks that are not on air so the live ones read at a glance. */
  dimIdleDecks: boolean
  /** Give the decks that are on air more of the height in the stacked view,
   *  so the live track is the biggest thing on screen. */
  enlargeOnAirDeck: boolean
  /** Which signal decides a deck is live — the mixer's on-air flag or
   *  playback — see `lib/deck-activity.ts`. */
  onAirSource: OnAirSource
  /** Renderer zoom factor, for booth screens from 13" to 27". */
  uiScale: number
  /** Whether the header shows the whole running order or only what is around now. */
  lineupView: LineupView
  /** Keep the window above every other application. */
  alwaysOnTop: boolean
  /** Hold off the screen saver while a set is running. */
  preventSleep: boolean
  /** Whether track downloading from the deck's media is enabled (hidden by default). */
  allowTrackDownload: boolean
  /** Protocol the timecode generator sends to; the deck it reads from is
   *  picked from the header, not here — see `state/timecode.ts`. */
  timecodeProtocol: TimecodeProtocolChoice
  timecodeFrameRate: FrameRate
  /** Whether the timecode source follows the on-air deck by itself instead of
   *  the manual pick in the header — see `state/timecode-follow.ts`. */
  timecodeFollowOnAir: boolean
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
  /** Whether the remote web view server is served over HTTPS/TLS. */
  remoteUseTls: boolean
  /** Maximum number of concurrent client devices allowed to connect. */
  remoteMaxClients: number
  /** Whether the remote view demands the access token carried in its link.
   *  Off, anyone who reaches the port gets in — meant for a private booth
   *  network, where a plain bookmarkable URL is worth more than the gate. */
  remoteRequireToken: boolean
  /** Name of the network adapter Pro DJ Link presence binds to — picked from
   *  `list_network_interfaces`, not typed in. Empty means automatic: the
   *  adapter on the first player's subnet. */
  prolinkInterface: string
  /** Whether the waveform view shows the deck number badge. */
  showDeckNumber: boolean
  /** Whether the deck bar shows the track title and artist. */
  showDeckTrackInfo: boolean
  /** Whether the deck bar shows the key/genre tags. */
  showDeckTrackTags: boolean
  /** Whether the deck bar shows the timecode-source (TC) button. */
  showDeckTcButton: boolean
  /** Whether the deck bar shows the play/pause transport glyph. */
  showDeckTransport: boolean
  /** Whether the deck bar shows the ON AIR / MASTER / SYNC badges. */
  showDeckSyncBadges: boolean
  /** Whether the stats row shows the elapsed time. */
  showDeckTime: boolean
  /** Whether the stats row shows the pitch percentage. */
  showDeckPitch: boolean
  /** Whether the time stat shows the time remaining underneath. */
  showDeckTimeRemaining: boolean
  /** Whether the stats row shows the BPM. */
  showDeckBpm: boolean
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
  /** Operator-chosen names shown next to the player number, e.g. the mixer
   *  channel a player is plugged into. A missing entry means no label. */
  deckLabels: DeckLabels
}

export const DEFAULT_SETTINGS: Settings = {
  theme: DEFAULT_THEME,
  waveformSaturation: 1,
  waveformMirrored: true,
  waveformSmoothing: true,
  waveformPlayheadPosition: DEFAULT_WAVEFORM_PLAYHEAD_POSITION,
  dimIdleDecks: true,
  enlargeOnAirDeck: false,
  onAirSource: DEFAULT_ON_AIR_SOURCE,
  uiScale: 1,
  lineupView: 'full',
  alwaysOnTop: false,
  preventSleep: false,
  allowTrackDownload: false,
  timecodeProtocol: 'off',
  timecodeFrameRate: 'smpte30',
  timecodeFollowOnAir: false,
  timecodeArtNetInterface: NO_NETWORK_INTERFACE,
  timecodeArtNetPort: ART_NET_DEFAULT_PORT,
  timecodeMtcPort: NO_MIDI_PORT,
  timecodeLtcDevice: NO_LTC_DEVICE,
  remoteEnabled: false,
  remotePort: REMOTE_DEFAULT_PORT,
  remoteInterface: NO_NETWORK_INTERFACE,
  remoteUseTls: false,
  remoteMaxClients: REMOTE_DEFAULT_MAX_CLIENTS,
  remoteRequireToken: true,
  prolinkInterface: NO_NETWORK_INTERFACE,
  ...FULL_DECK_VIEW,
  deckCueListWidth: 168,
  deckEventListWidth: 152,
  deckLabels: {}
}

interface SettingsActions {
  update<K extends keyof Settings>(key: K, value: Settings[K]): void
  resetAll(): void
  applyDeckViewPreset(preset: DeckViewPreset): void
}

const SETTINGS_STORAGE_KEY = 'decktector.settings'

/**
 * User preferences, persisted as they change. Nothing here reaches the outside
 * world on its own — `connectSettingsToApp` is what applies them.
 */
export const useSettings = create<Settings & SettingsActions>()(
  persist(
    (set) => ({
      ...DEFAULT_SETTINGS,
      update: (key, value) => set({ [key]: value } as Partial<Settings>),
      resetAll: () => set({ ...DEFAULT_SETTINGS }),
      applyDeckViewPreset: (preset) => set({ ...DECK_VIEW_PRESETS[preset].values })
    }),
    {
      name: SETTINGS_STORAGE_KEY,
      // Actions are not state: keeping them out avoids writing dead keys to storage.
      partialize: (state) => pickSettings(state),
      // Storage is an outside boundary: re-validate what was written before
      // trusting it (hand-edited, or left by an older build).
      merge: (persisted, current) => mergePersistedSettings(persisted, current)
    }
  )
)

/** Overlays what storage held onto `current`, re-validating the fields that have a closed set of values. */
export function mergePersistedSettings<T extends Settings>(persisted: unknown, current: T): T {
  const stored = (persisted ?? {}) as Partial<Settings>
  return {
    ...current,
    ...stored,
    theme: parseTheme(stored.theme),
    onAirSource: parseOnAirSource(stored.onAirSource),
    waveformPlayheadPosition: parseWaveformPlayheadPosition(stored.waveformPlayheadPosition),
    deckLabels: sanitizeDeckLabels(stored.deckLabels)
  }
}

function pickSettings(state: Settings): Settings {
  const {
    theme,
    waveformSaturation,
    waveformMirrored,
    waveformSmoothing,
    waveformPlayheadPosition,
    dimIdleDecks,
    enlargeOnAirDeck,
    onAirSource,
    uiScale,
    lineupView,
    alwaysOnTop,
    preventSleep,
    allowTrackDownload,
    timecodeProtocol,
    timecodeFrameRate,
    timecodeFollowOnAir,
    timecodeArtNetInterface,
    timecodeArtNetPort,
    timecodeMtcPort,
    timecodeLtcDevice,
    remoteEnabled,
    remotePort,
    remoteInterface,
    remoteUseTls,
    remoteMaxClients,
    remoteRequireToken,
    prolinkInterface,
    showDeckNumber,
    showDeckTrackInfo,
    showDeckTrackTags,
    showDeckTcButton,
    showDeckTransport,
    showDeckSyncBadges,
    showDeckTime,
    showDeckPitch,
    showDeckTimeRemaining,
    showDeckBpm,
    showDeckCueList,
    showDeckEventList,
    showDeckArtwork,
    showDeckWaveformOverview,
    showDeckWaveformDetail,
    deckCueListWidth,
    deckEventListWidth,
    deckLabels
  } = state
  return {
    theme,
    waveformSaturation,
    waveformMirrored,
    waveformSmoothing,
    waveformPlayheadPosition,
    dimIdleDecks,
    enlargeOnAirDeck,
    onAirSource,
    uiScale,
    lineupView,
    alwaysOnTop,
    preventSleep,
    allowTrackDownload,
    timecodeProtocol,
    timecodeFrameRate,
    timecodeFollowOnAir,
    timecodeArtNetInterface,
    timecodeArtNetPort,
    timecodeMtcPort,
    timecodeLtcDevice,
    remoteEnabled,
    remotePort,
    remoteInterface,
    remoteUseTls,
    remoteMaxClients,
    remoteRequireToken,
    prolinkInterface,
    showDeckNumber,
    showDeckTrackInfo,
    showDeckTrackTags,
    showDeckTcButton,
    showDeckTransport,
    showDeckSyncBadges,
    showDeckTime,
    showDeckPitch,
    showDeckTimeRemaining,
    showDeckBpm,
    showDeckCueList,
    showDeckEventList,
    showDeckArtwork,
    showDeckWaveformOverview,
    showDeckWaveformDetail,
    deckCueListWidth,
    deckEventListWidth,
    deckLabels
  }
}

/** The main window's Deck view toggles, re-rendering only when one of them changes. */
export function useDeckViewToggles(): DeckViewToggles {
  return useSettings(useShallow(pickDeckViewToggles))
}
