import type { DeckViewToggles } from '../state/settings'

export interface DeckViewToggleRow {
  key: keyof DeckViewToggles
  label: string
  hint: string
}

export interface DeckViewToggleGroup {
  title: string
  rows: readonly DeckViewToggleRow[]
}

/**
 * The Deck view toggles as the operator sees them, in display order. Shared
 * by the settings tab (the main window) and a deck window's own display
 * options, so both list the same pieces under the same names.
 */
export const DECK_VIEW_TOGGLE_GROUPS: readonly DeckViewToggleGroup[] = [
  {
    title: 'Deck bar',
    rows: [
      {
        key: 'showDeckArtwork',
        label: 'Artwork',
        hint: 'Album art thumbnail next to the track info'
      },
      {
        key: 'showDeckTrackInfo',
        label: 'Track title & artist',
        hint: "The track's title, artist and album"
      },
      {
        key: 'showDeckTrackTags',
        label: 'Track tags',
        hint: 'Key and genre badges next to the track title'
      },
      {
        key: 'showDeckTcButton',
        label: 'TC button',
        hint: 'Button to send timecode from this deck'
      },
      { key: 'showDeckTransport', label: 'Play/pause button', hint: 'The transport state glyph' },
      {
        key: 'showDeckSyncBadges',
        label: 'Sync badges',
        hint: 'ON AIR, MASTER and SYNC indicators'
      },
      {
        key: 'showDeckTime',
        label: 'Time',
        hint: 'Elapsed time, as its own column next to pitch and BPM'
      },
      {
        key: 'showDeckPitch',
        label: 'Pitch',
        hint: 'Pitch percentage as its own column, next to time and BPM'
      },
      {
        key: 'showDeckTimeRemaining',
        label: 'Time remaining',
        hint: 'Countdown under the elapsed time readout'
      },
      { key: 'showDeckBpm', label: 'BPM', hint: 'BPM as its own column, next to time and pitch' }
    ]
  },
  {
    title: 'Deck body',
    rows: [
      { key: 'showDeckCueList', label: 'Cue list', hint: 'Cue points below the track info' },
      {
        key: 'showDeckEventList',
        label: 'Track structure',
        hint: 'Intro, builds, drops and breaks as they come up'
      },
      {
        key: 'showDeckWaveformOverview',
        label: 'Waveform overview',
        hint: 'Full-track waveform strip'
      },
      {
        key: 'showDeckWaveformDetail',
        label: 'Waveform detail',
        hint: 'Zoomed-in waveform around the playhead'
      },
      {
        key: 'showDeckNumber',
        label: 'Deck number',
        hint: "The deck's number badge in the waveform view"
      }
    ]
  }
]
