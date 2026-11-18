/**
 * Timecode values for the show: start timecodes are typed and stored as
 * `HH:MM:SS:FF` labels, and only become frame counts when they are pushed to
 * the timecode service. This module is the one place a label turns into
 * frames, so a show keeps the exact text the operator entered whatever its
 * frame rate does later.
 *
 * Trims are stored in milliseconds — the unit the service adds to a deck's
 * position — and shown and stepped in frames.
 */

import type { FrameRate } from '../model'

/** The frame rates a show can run at; drop-frame is not supported. */
export type ShowFps = 24 | 25 | 30

export const SHOW_FPS_OPTIONS: readonly ShowFps[] = [24, 25, 30]

/** The timecode service's name for each show rate. */
const FRAME_RATE_OF_FPS: Readonly<Record<ShowFps, FrameRate>> = {
  24: 'film24',
  25: 'ebu25',
  30: 'smpte30'
}

/** `fps` as the timecode service names it. */
export function frameRateOfFps(fps: ShowFps): FrameRate {
  return FRAME_RATE_OF_FPS[fps]
}

/** The largest trim either way: far beyond any cue nudge, short of a mistake. */
export const MAX_TRIM_MS = 10_000

/** Where an unidentified track's timeline starts by default — a label valid at every rate. */
export const FALLBACK_START_TC_DEFAULT = '23:00:00:00'

const HOURS_PER_DAY = 24
const MINUTES_PER_HOUR = 60
const SECONDS_PER_MINUTE = 60
const SECONDS_PER_HOUR = MINUTES_PER_HOUR * SECONDS_PER_MINUTE
const MS_PER_SECOND = 1000

/** Two digits per field, colon-separated: the only shape a label may take. */
const TIMECODE_LABEL_PATTERN = /^(\d{2}):(\d{2}):(\d{2}):(\d{2})$/

/** The four fields of a well-formed `HH:MM:SS:FF` label. */
export interface TimecodeFields {
  hours: number
  minutes: number
  seconds: number
  frames: number
}

/**
 * The fields of `text` if it is a well-formed label — hours 0-23, minutes and
 * seconds 0-59, a two-digit frame field — at any frame rate. Whether the
 * frame field fits a given rate is {@link parseTimecode}'s question: a label
 * stays well-formed when the show's rate changes under it.
 */
export function readTimecodeLabel(text: string): TimecodeFields | null {
  const match = TIMECODE_LABEL_PATTERN.exec(text)
  if (match === null) return null
  const [hours, minutes, seconds, frames] = match.slice(1).map(Number)
  if (hours >= HOURS_PER_DAY || minutes >= MINUTES_PER_HOUR || seconds >= SECONDS_PER_MINUTE) {
    return null
  }
  return { hours, minutes, seconds, frames }
}

/** How many frames a timecode clock counts before it wraps to `00:00:00:00`. */
export function framesPerDay(fps: ShowFps): number {
  return HOURS_PER_DAY * SECONDS_PER_HOUR * fps
}

/**
 * Frames from `00:00:00:00` to the label `text` at `fps`, or null when the
 * label is malformed or its frame field is not below `fps`.
 */
export function parseTimecode(text: string, fps: ShowFps): number | null {
  const fields = readTimecodeLabel(text)
  if (fields === null || fields.frames >= fps) return null
  const seconds =
    fields.hours * SECONDS_PER_HOUR + fields.minutes * SECONDS_PER_MINUTE + fields.seconds
  return seconds * fps + fields.frames
}

function twoDigits(value: number): string {
  return String(value).padStart(2, '0')
}

/** The `HH:MM:SS:FF` label `frames` after `00:00:00:00` at `fps`, wrapping at 24 hours. */
export function formatTimecode(frames: number, fps: ShowFps): string {
  const day = framesPerDay(fps)
  const wrapped = ((Math.trunc(frames) % day) + day) % day
  const totalSeconds = Math.floor(wrapped / fps)
  return [
    Math.floor(totalSeconds / SECONDS_PER_HOUR),
    Math.floor(totalSeconds / SECONDS_PER_MINUTE) % MINUTES_PER_HOUR,
    totalSeconds % SECONDS_PER_MINUTE,
    wrapped % fps
  ]
    .map(twoDigits)
    .join(':')
}

/** The nearest whole number of frames `ms` of trim amounts to at `fps`. */
export function trimMsToFrames(ms: number, fps: ShowFps): number {
  return Math.round((ms * fps) / MS_PER_SECOND)
}

/**
 * `frames` of trim in whole milliseconds, as stored. A frame at 30 fps is not
 * a whole number of milliseconds; rounding keeps the stored value within half
 * a millisecond, and {@link trimMsToFrames} brings it back to the same frame.
 */
export function trimFramesToMs(frames: number, fps: ShowFps): number {
  return Math.round((frames * MS_PER_SECOND) / fps)
}
