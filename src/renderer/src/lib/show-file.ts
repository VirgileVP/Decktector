/**
 * The show's file format, for moving a night between machines. Plain JSON,
 * tagged with its own format name and version like the lineup file, and kept
 * separate from it: the lineup says who plays when, the show which timecode
 * each track runs on.
 *
 * Start timecodes are written and read as the `HH:MM:SS:FF` label itself — no
 * frame arithmetic happens here. A label's shape is checked, but a frame
 * field at or above the file's fps is kept: a show saved after an fps change
 * must load again, and `analyseShow` flags the row.
 *
 * Row ids are left out: they only keep the editor's rows stable within a
 * session, and an imported show gets fresh ones.
 */

import { localDayStamp } from './file-date'
import { isRecord } from './is-record'
import { generateId } from './lineup'
import type { ShowData, ShowRow } from './show'
import { MAX_TRIM_MS, SHOW_FPS_OPTIONS, readTimecodeLabel, type ShowFps } from './show-timecode'

export const SHOW_FILE_FORMAT = 'decktector.show'
export const SHOW_FILE_VERSION = 1

/** Far beyond any real night; guards against a file that is not a show at all. */
export const MAX_IMPORTED_ROWS = 2000
/** Longer than any real title or artist name. */
export const MAX_TEXT_LENGTH = 300
/** A track longer than a day is not a track. */
const MAX_DURATION_SECONDS = 24 * 3600

interface ShowFileRow {
  title: string
  artist: string
  durationSeconds: number
  startTc: string | null
  trimMs: number
  setArtist?: string
  absent: boolean
}

interface ShowFile {
  format: typeof SHOW_FILE_FORMAT
  version: typeof SHOW_FILE_VERSION
  fps: ShowFps
  fallbackStartTc: string
  cutWhenUnidentified: boolean
  rows: ShowFileRow[]
}

export type ShowFileResult = { ok: true; show: ShowData } | { ok: false; error: string }

function toFileRow(row: ShowRow): ShowFileRow {
  const { title, artist, durationSeconds, startTimecode, trimMs, setArtist, absent } = row
  return {
    title,
    artist,
    durationSeconds,
    startTc: startTimecode,
    trimMs,
    ...(setArtist === undefined ? {} : { setArtist }),
    absent
  }
}

/** The show as file text, rows in show order. */
export function serializeShow(show: ShowData): string {
  const file: ShowFile = {
    format: SHOW_FILE_FORMAT,
    version: SHOW_FILE_VERSION,
    fps: show.fps,
    fallbackStartTc: show.fallbackStartTimecode,
    cutWhenUnidentified: show.cutWhenUnidentified,
    rows: show.rows.map(toFileRow)
  }
  return `${JSON.stringify(file, null, 2)}\n`
}

/** The show file's extension: "Decktector show", JSON inside. */
export const SHOW_FILE_EXTENSION = 'dshow'

/** A suggested file name for today's export, e.g. `show-2026-10-09.dshow`. */
export function showFileName(date: Date): string {
  return `show-${localDayStamp(date)}.${SHOW_FILE_EXTENSION}`
}

class ShowFileError extends Error {}

function readText(value: unknown, label: string, { required }: { required: boolean }): string {
  if (typeof value !== 'string') throw new ShowFileError(`${label} is missing.`)
  if (required && value.trim().length === 0) throw new ShowFileError(`${label} is empty.`)
  if (value.length > MAX_TEXT_LENGTH) throw new ShowFileError(`${label} is too long.`)
  return value
}

function readLabel(value: unknown, label: string): string {
  if (typeof value !== 'string' || readTimecodeLabel(value) === null) {
    throw new ShowFileError(`${label} is not a valid HH:MM:SS:FF timecode.`)
  }
  return value
}

function readInteger(value: unknown, label: string, min: number, max: number): number {
  if (typeof value !== 'number' || !Number.isInteger(value) || value < min || value > max) {
    throw new ShowFileError(`${label} is not a whole number from ${min} to ${max}.`)
  }
  return value
}

function readRow(value: unknown, index: number): ShowRow {
  const label = `Row ${index + 1}`
  if (!isRecord(value)) throw new ShowFileError(`${label} is not a track.`)
  if (value.absent !== undefined && typeof value.absent !== 'boolean') {
    throw new ShowFileError(`${label} has an invalid absent flag.`)
  }

  const row: ShowRow = {
    id: generateId(),
    title: readText(value.title, `${label} title`, { required: true }),
    artist: readText(value.artist, `${label} artist`, { required: false }),
    durationSeconds: readInteger(
      value.durationSeconds,
      `${label} duration`,
      0,
      MAX_DURATION_SECONDS
    ),
    startTimecode:
      value.startTc === null || value.startTc === undefined
        ? null
        : readLabel(value.startTc, `${label} start`),
    trimMs: readInteger(value.trimMs, `${label} trim`, -MAX_TRIM_MS, MAX_TRIM_MS),
    absent: value.absent === true
  }
  if (value.setArtist !== undefined) {
    row.setArtist = readText(value.setArtist, `${label} set artist`, { required: false })
  }
  return row
}

function readFps(value: unknown): ShowFps {
  const fps = SHOW_FPS_OPTIONS.find((option) => option === value)
  if (fps === undefined) throw new ShowFileError('The show file has an invalid frame rate.')
  return fps
}

function readShow(value: unknown): ShowData {
  if (!isRecord(value) || value.format !== SHOW_FILE_FORMAT) {
    throw new ShowFileError('This is not a Decktector show file.')
  }
  if (typeof value.version === 'number' && value.version > SHOW_FILE_VERSION) {
    throw new ShowFileError(
      'This show file comes from a newer version of Decktector — update the app to open it.'
    )
  }
  if (value.version !== SHOW_FILE_VERSION) {
    throw new ShowFileError('The show file has an unknown version.')
  }
  if (typeof value.cutWhenUnidentified !== 'boolean') {
    throw new ShowFileError('The show file does not say whether to cut unidentified tracks.')
  }
  if (!Array.isArray(value.rows)) throw new ShowFileError('The show file has no rows.')
  if (value.rows.length > MAX_IMPORTED_ROWS) {
    throw new ShowFileError('The show file holds too many rows.')
  }

  return {
    fps: readFps(value.fps),
    fallbackStartTimecode: readLabel(value.fallbackStartTc, 'The fallback start'),
    cutWhenUnidentified: value.cutWhenUnidentified,
    rows: value.rows.map(readRow)
  }
}

/**
 * Reads a show file back. Everything in it is checked, since it may have
 * been edited by hand or be some other JSON file picked by mistake; fields
 * this version does not know are ignored.
 */
export function parseShowFile(text: string): ShowFileResult {
  let json: unknown
  try {
    json = JSON.parse(text)
  } catch {
    return { ok: false, error: 'This file is not valid JSON.' }
  }

  try {
    return { ok: true, show: readShow(json) }
  } catch (error) {
    if (error instanceof ShowFileError) return { ok: false, error: error.message }
    throw error
  }
}
