/**
 * The show: tonight's tracks, each with the timecode its own timeline starts
 * at. The console runs one timeline per track, so a cue programmed against a
 * song fires on that song whatever was played before it.
 *
 * Start timecodes are `HH:MM:SS:FF` labels, kept exactly as typed and never as
 * frame counts (see `lib/show-timecode`): changing the show's frame rate keeps
 * every label, and a label whose frames no longer fit is flagged, not rewritten.
 */

import { generateId } from './lineup'
import type { ShowFps } from './show-timecode'

export interface ShowRow {
  /** Session-only, regenerated on import; never written to a file. */
  id: string
  title: string
  /** The track's own artist — part of the matching key. */
  artist: string
  /** Whole seconds; 0 when unknown, and an unknown track never matches. */
  durationSeconds: number
  /** The `HH:MM:SS:FF` label as typed; null while blank. Never a frame count. */
  startTimecode: string | null
  /** Signed nudge in ms: positive runs the timecode ahead, so cues fire earlier. */
  trimMs: number
  /** Who plays the set the row belongs to; never read by matching. */
  setArtist?: string
  /** The row was missing from the last playlist merged in; kept, not deleted. */
  absent: boolean
}

export interface ShowData {
  fps: ShowFps
  /** Where an unidentified track's timeline starts — a label, like the rows' starts. */
  fallbackStartTimecode: string
  /** Stops the output for an unidentified track instead of running from the fallback. */
  cutWhenUnidentified: boolean
  rows: ShowRow[]
}

/** A new row for a track, blank start and no trim. */
export function createRow(partial: Pick<ShowRow, 'title' | 'artist' | 'durationSeconds'>): ShowRow {
  return { id: generateId(), ...partial, startTimecode: null, trimMs: 0, absent: false }
}
