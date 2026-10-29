import type { TrackCue } from '@renderer/model'

/** The cue stored on a hot cue pad, if the track has one there. */
export function hotCueFor(cues: readonly TrackCue[], button: string): TrackCue | null {
  return cues.find((cue) => cue.button === button) ?? null
}
