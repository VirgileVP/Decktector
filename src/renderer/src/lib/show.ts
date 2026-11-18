/**
 * The show: tonight's tracks, each with the timecode its own timeline starts
 * at. The console runs one timeline per track, so a cue programmed against a
 * song fires on that song whatever was played before it.
 *
 * Start timecodes are `HH:MM:SS:FF` labels, kept exactly as typed and never as
 * frame counts (see `lib/show-timecode`): changing the show's frame rate keeps
 * every label, and a label whose frames no longer fit is flagged, not rewritten.
 */

import { isRecord } from './is-record'
import { generateId } from './lineup'
import { FALLBACK_START_TC_DEFAULT, SHOW_FPS_OPTIONS, type ShowFps } from './show-timecode'

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

/**
 * A new show's frame rate: the timecode output's own default, so a first
 * show does not start out at a rate the output is not running.
 */
export const DEFAULT_SHOW_FPS: ShowFps = 30

/** A new row for a track, blank start and no trim. */
export function createRow(partial: Pick<ShowRow, 'title' | 'artist' | 'durationSeconds'>): ShowRow {
  return { id: generateId(), ...partial, startTimecode: null, trimMs: 0, absent: false }
}

/** A new show holding `rows`, with the default fallback start and no cut. */
export function createShow(rows: ShowRow[], fps: ShowFps = DEFAULT_SHOW_FPS): ShowData {
  return { fps, fallbackStartTimecode: FALLBACK_START_TC_DEFAULT, cutWhenUnidentified: false, rows }
}

function isShowRow(value: unknown): value is ShowRow {
  return (
    isRecord(value) &&
    typeof value.id === 'string' &&
    typeof value.title === 'string' &&
    typeof value.artist === 'string' &&
    typeof value.durationSeconds === 'number' &&
    (value.startTimecode === null || typeof value.startTimecode === 'string') &&
    typeof value.trimMs === 'number' &&
    (value.setArtist === undefined || typeof value.setArtist === 'string') &&
    typeof value.absent === 'boolean'
  )
}

/**
 * Whether `value` has the shape of a {@link ShowData} — the check on a show
 * read back from storage, which an older build or a hand edit may have left
 * in another shape. A show file goes through `lib/show-file` instead, which
 * checks every value, not only the shape.
 */
export function isShowData(value: unknown): value is ShowData {
  return (
    isRecord(value) &&
    SHOW_FPS_OPTIONS.some((fps) => fps === value.fps) &&
    typeof value.fallbackStartTimecode === 'string' &&
    typeof value.cutWhenUnidentified === 'boolean' &&
    Array.isArray(value.rows) &&
    value.rows.every(isShowRow)
  )
}
