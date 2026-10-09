/**
 * The lineup's file format, for moving a night between machines or keeping
 * one for later. Plain JSON, tagged with its own format name and version so a
 * file can be recognized — and a future layout migrated — without guessing.
 *
 * Slot ids are left out: they only keep the editor's rows stable within a
 * session, and an imported lineup gets fresh ones.
 */

import { isRecord } from './is-record'
import { AXIS_MINUTES, type AxisMinutes } from './lineup-axis'
import { generateId, type LineupData, type LineupSlot } from './lineup'

export const LINEUP_FILE_FORMAT = 'decktector.lineup'
export const LINEUP_FILE_VERSION = 1

/** Far beyond any real night; guards against a file that is not a lineup at all. */
const MAX_IMPORTED_SLOTS = 500

interface LineupFileSlot {
  artist: string
  note: string
  startOffset: AxisMinutes | null
}

interface LineupFile {
  format: typeof LINEUP_FILE_FORMAT
  version: typeof LINEUP_FILE_VERSION
  doorsOffset: AxisMinutes | null
  endOffset: AxisMinutes | null
  slots: LineupFileSlot[]
}

export type LineupFileResult = { ok: true; lineup: LineupData } | { ok: false; error: string }

/** The lineup as file text, sets in running order. */
export function serializeLineup({ slots, endOffset, doorsOffset }: LineupData): string {
  const file: LineupFile = {
    format: LINEUP_FILE_FORMAT,
    version: LINEUP_FILE_VERSION,
    doorsOffset,
    endOffset,
    slots: slots.map(({ artist, note, startOffset }) => ({ artist, note, startOffset }))
  }
  return `${JSON.stringify(file, null, 2)}\n`
}

/** A suggested file name for today's export, e.g. `lineup-2026-10-09.json`. */
export function lineupFileName(date: Date): string {
  const day = [
    date.getFullYear(),
    String(date.getMonth() + 1).padStart(2, '0'),
    String(date.getDate()).padStart(2, '0')
  ].join('-')
  return `lineup-${day}.json`
}

class LineupFileError extends Error {}

function readOffset(value: unknown, field: string): AxisMinutes | null {
  if (value === null || value === undefined) return null
  if (typeof value !== 'number' || !Number.isInteger(value) || value < 0 || value > AXIS_MINUTES) {
    throw new LineupFileError(`"${field}" is not a valid time.`)
  }
  return value
}

function readSlot(value: unknown, index: number): LineupSlot {
  const label = `Set ${index + 1}`
  if (!isRecord(value)) throw new LineupFileError(`${label} is not a set.`)
  if (typeof value.artist !== 'string') throw new LineupFileError(`${label} has no artist.`)
  if (value.note !== undefined && typeof value.note !== 'string') {
    throw new LineupFileError(`${label} has an invalid note.`)
  }

  return {
    id: generateId(),
    artist: value.artist,
    note: value.note ?? '',
    startOffset: readOffset(value.startOffset, `${label} start`)
  }
}

function readLineup(value: unknown): LineupData {
  if (!isRecord(value) || value.format !== LINEUP_FILE_FORMAT) {
    throw new LineupFileError('This is not a Decktector lineup file.')
  }
  if (typeof value.version === 'number' && value.version > LINEUP_FILE_VERSION) {
    throw new LineupFileError(
      'This lineup file comes from a newer version of Decktector — update the app to open it.'
    )
  }
  if (value.version !== LINEUP_FILE_VERSION) {
    throw new LineupFileError('The lineup file has an unknown version.')
  }
  if (!Array.isArray(value.slots)) throw new LineupFileError('The lineup file has no sets.')
  if (value.slots.length > MAX_IMPORTED_SLOTS) {
    throw new LineupFileError('The lineup file holds too many sets.')
  }

  return {
    slots: value.slots.map(readSlot),
    endOffset: readOffset(value.endOffset, 'endOffset'),
    doorsOffset: readOffset(value.doorsOffset, 'doorsOffset')
  }
}

/**
 * Reads a lineup file back. Everything in it is checked, since it may have
 * been edited by hand or be some other JSON file picked by mistake.
 */
export function parseLineupFile(text: string): LineupFileResult {
  let json: unknown
  try {
    json = JSON.parse(text)
  } catch {
    return { ok: false, error: 'This file is not valid JSON.' }
  }

  try {
    return { ok: true, lineup: readLineup(json) }
  } catch (error) {
    if (error instanceof LineupFileError) return { ok: false, error: error.message }
    throw error
  }
}
