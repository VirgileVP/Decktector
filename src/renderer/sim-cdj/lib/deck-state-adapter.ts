import type { DeckState } from '@renderer/model'
import type { SimDeckView } from '@renderer/model/SimDeckView'

/**
 * The shared waveform canvases read a `DeckState` from the renderer's main
 * store and interpolate the playhead from it. This maps the simulator's
 * authoritative deck view onto that shape, anchored on the server's clock.
 * The canvases never ask which media a track lives on or how far its
 * position can be trusted, so `loadedTrack` and `positionQuality` are left
 * unknown here; the engine's own deck state carries both.
 */
export function toDeckState(view: SimDeckView, reportedAt: number): DeckState {
  return {
    deckId: view.deckId,
    playState: view.playState,
    isMaster: view.isMaster,
    isSynced: view.isSynced,
    isOnAir: view.isOnAir,
    pitch: view.effectivePitch,
    trackBpm: view.trackBpm,
    effectiveBpm: view.effectiveBpm,
    beatInMeasure: view.beatInMeasure,
    trackId: view.trackId,
    loadedTrack: null,
    playheadMs: view.trackIndex !== null ? view.positionMs : null,
    positionQuality: null,
    reportedAt,
    activeLoop:
      view.loopState?.active === true
        ? { range: { startMs: view.loopState.startMs, endMs: view.loopState.endMs } }
        : null
  }
}
