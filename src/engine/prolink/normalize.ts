import { CDJStatus } from 'prolink-connect'

import type { DeckState, PlayState } from '../types'

const PLAY_STATE_MAP: Record<CDJStatus.PlayState, PlayState> = {
  [CDJStatus.PlayState.Empty]: 'empty',
  [CDJStatus.PlayState.Loading]: 'loading',
  [CDJStatus.PlayState.Playing]: 'playing',
  [CDJStatus.PlayState.Looping]: 'playing',
  [CDJStatus.PlayState.Paused]: 'paused',
  [CDJStatus.PlayState.Cued]: 'cued',
  [CDJStatus.PlayState.Cuing]: 'cued',
  [CDJStatus.PlayState.PlatterHeld]: 'paused',
  [CDJStatus.PlayState.Searching]: 'searching',
  [CDJStatus.PlayState.SpunDown]: 'searching',
  [CDJStatus.PlayState.Ended]: 'ended'
}

export function mapPlayState(state: CDJStatus.PlayState): PlayState {
  return PLAY_STATE_MAP[state] ?? 'empty'
}

/** True when the playhead is actually advancing. */
export function isAdvancing(state: CDJStatus.State): boolean {
  return (
    state.playState === CDJStatus.PlayState.Playing ||
    state.playState === CDJStatus.PlayState.Looping
  )
}

/**
 * Maps a raw CDJ status packet to the normalized DeckState.
 * `playheadMs` comes from the PositionTracker, which owns that estimation.
 */
export function normalizeStatus(
  state: CDJStatus.State,
  playheadMs: number | null,
  reportedAt: number
): DeckState {
  const trackBpm = state.trackBPM
  return {
    deckId: state.deviceId,
    playState: mapPlayState(state.playState),
    isMaster: state.isMaster,
    isSynced: state.isSync,
    isOnAir: state.isOnAir,
    pitch: state.sliderPitch,
    trackBpm,
    effectiveBpm: trackBpm !== null ? trackBpm * (1 + state.sliderPitch / 100) : null,
    beatInMeasure: state.beatInMeasure > 0 ? state.beatInMeasure : null,
    trackId: state.trackId !== 0 ? state.trackId : null,
    playheadMs,
    reportedAt
  }
}
