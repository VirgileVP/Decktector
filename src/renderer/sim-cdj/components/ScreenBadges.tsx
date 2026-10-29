import type { SimDeckView } from '@renderer/model/SimDeckView'

import { TEMPO_RANGE_OPTIONS } from '../lib/constants'
import { useAudioStore, useDeckAudioStatus } from '../state/audio-store'

const PLAY_STATE_LABELS: Record<SimDeckView['playState'], string> = {
  empty: 'NO TRACK',
  loading: 'LOADING',
  cued: 'CUE',
  playing: 'PLAY',
  paused: 'PAUSE',
  searching: 'SEARCH',
  ended: 'END'
}

/** Shown only while a loaded track has no sound to play; nothing once it is ready. */
const AUDIO_STATUS_LABELS = {
  loading: 'LOADING',
  none: 'NO AUDIO',
  error: 'AUDIO ERROR'
} as const

interface Badge {
  label: string
  lit: boolean
  className: string
}

/** The lamps along the screen's edge: transport, master/sync/on-air, loop and tempo range. */
export function ScreenBadges({ view }: { view: SimDeckView }): React.JSX.Element {
  const tempoLabel =
    TEMPO_RANGE_OPTIONS.find((option) => option.range === view.tempoRange)?.label ?? view.tempoRange
  const audioStatus = useDeckAudioStatus(view.deckId)
  const soundState = useAudioStore((state) => state.sound)
  const badges: Badge[] = [
    { label: 'MASTER', lit: view.isMaster, className: 'badge-master' },
    { label: 'SYNC', lit: view.isSynced, className: 'badge-sync' },
    { label: 'ON AIR', lit: view.isOnAir, className: 'badge-onair' },
    { label: 'LOOP', lit: view.loopState?.active === true, className: 'badge-loop' }
  ]
  return (
    <div className="screen-badges">
      <span className="screen-state">{PLAY_STATE_LABELS[view.playState]}</span>
      {/* Before the first click no audio has been fetched yet; the page banner says why. */}
      {view.trackIndex !== null && soundState === 'running' && audioStatus !== 'ready' && (
        <span className={`screen-audio-status screen-audio-${audioStatus}`}>
          {AUDIO_STATUS_LABELS[audioStatus]}
        </span>
      )}
      {badges.map((badge) => (
        <span
          key={badge.label}
          className={`screen-badge ${badge.className}${badge.lit ? ' screen-badge-lit' : ''}`}
        >
          {badge.label}
        </span>
      ))}
      <span className="screen-range">TEMPO {tempoLabel}</span>
    </div>
  )
}
