/**
 * What is wrong with a show, as data the editor turns into badges and a
 * readiness line, plus the two bulk edits that fix the common cases.
 *
 * A **conflict** makes the show not ready: two rows' ranges overlap, a range
 * runs past midnight, the same track is listed twice with different (or
 * blank) starts, or a start's frame field does not fit the show's fps. A
 * **warning** does not: a blank start (the track falls back at runtime) and a
 * row missing from the last playlist merged in.
 *
 * A row's range is `[start, start + duration)` in frames. A row with an
 * unknown duration has no range, and a row whose label does not parse at the
 * show's fps is left out of range checks: its invalid frames are the conflict.
 */

import type { ShowData, ShowRow } from './show'
import { identityMatches, normaliseText } from './show-identity'
import { formatTimecode, framesPerDay, parseTimecode, type ShowFps } from './show-timecode'

export type RowIssue =
  | { kind: 'overlap'; withRowId: string }
  | { kind: 'crossesMidnight' }
  | { kind: 'duplicate'; groupSize: number; sameStart: boolean }
  | { kind: 'absent' }
  | { kind: 'blankStart' }
  /** The label's frame field is not below the show's fps (after an fps change). */
  | { kind: 'invalidFrames' }

export interface ShowAnalysis {
  /** Issues per row id; a row with none has no entry. */
  byRow: Map<string, RowIssue[]>
  /** Overlapping pairs, midnight crossings, invalid starts and conflicting duplicate groups. */
  conflicts: number
  /** Blank starts and absent rows. */
  warnings: number
  ready: boolean
}

/** A row listed more than once: the same track by {@link identityMatches}. */
interface DuplicateGroup {
  rows: ShowRow[]
  /** Every copy has the same, non-blank start: they merge without losing anything. */
  sameStart: boolean
}

/** A row with a start and a known duration, in frames; `end` may run past midnight. */
interface TimedRow {
  id: string
  start: number
  end: number
}

function identityKey(row: ShowRow): string {
  return `${normaliseText(row.title)}\u0000${normaliseText(row.artist)}`
}

function hasSameStart(rows: readonly ShowRow[]): boolean {
  const [first, ...others] = rows
  return (
    first.startTimecode !== null && others.every((row) => row.startTimecode === first.startTimecode)
  )
}

/**
 * Groups of rows naming the same track, in show order within each group.
 *
 * The duration tolerance is not transitive (300 s matches 302 s, which
 * matches 304 s), so a group is a chain of matches: rows with the same title
 * and artist, sorted by duration, split wherever two neighbours do not match.
 */
function duplicateGroups(rows: readonly ShowRow[]): DuplicateGroup[] {
  const order = new Map(rows.map((row, index) => [row.id, index]))
  const byKey = new Map<string, ShowRow[]>()
  for (const row of rows) {
    if (row.durationSeconds <= 0) continue
    const key = identityKey(row)
    byKey.set(key, [...(byKey.get(key) ?? []), row])
  }

  const groups: DuplicateGroup[] = []
  for (const candidates of byKey.values()) {
    const byDuration = [...candidates].sort((a, b) => a.durationSeconds - b.durationSeconds)
    let chain: ShowRow[] = []
    const closeChain = (): void => {
      if (chain.length > 1) {
        const inShowOrder = chain.sort((a, b) => (order.get(a.id) ?? 0) - (order.get(b.id) ?? 0))
        groups.push({ rows: inShowOrder, sameStart: hasSameStart(inShowOrder) })
      }
    }
    for (const row of byDuration) {
      const previous = chain[chain.length - 1]
      if (previous !== undefined && !identityMatches(previous, row)) {
        closeChain()
        chain = []
      }
      chain.push(row)
    }
    closeChain()
  }
  return groups
}

/**
 * Every pair of timed rows whose ranges intersect. Ranges are half-open, so
 * one ending exactly where the next starts does not overlap it, and a range
 * running past midnight is checked again against the start of the day.
 */
function overlappingPairs(timed: readonly TimedRow[], day: number): [string, string][] {
  const sorted = [...timed].sort((a, b) => a.start - b.start)
  const seen = new Set<string>()
  const pairs: [string, string][] = []
  const report = (a: TimedRow, b: TimedRow): void => {
    const key = a.id < b.id ? `${a.id}|${b.id}` : `${b.id}|${a.id}`
    if (seen.has(key)) return
    seen.add(key)
    pairs.push([a.id, b.id])
  }

  sorted.forEach((row, index) => {
    for (let next = index + 1; next < sorted.length && sorted[next].start < row.end; next++) {
      report(row, sorted[next])
    }
    const wrappedEnd = row.end - day
    for (let early = 0; early < sorted.length && sorted[early].start < wrappedEnd; early++) {
      if (early !== index) report(row, sorted[early])
    }
  })
  return pairs
}

/** Everything wrong with `show`, row by row, and whether it is ready to run. */
export function analyseShow(show: ShowData): ShowAnalysis {
  const byRow = new Map<string, RowIssue[]>()
  const flag = (rowId: string, issue: RowIssue): void => {
    byRow.set(rowId, [...(byRow.get(rowId) ?? []), issue])
  }
  let conflicts = 0
  let warnings = 0

  const mergeableGroupOf = new Map<string, DuplicateGroup>()
  for (const group of duplicateGroups(show.rows)) {
    for (const row of group.rows) {
      flag(row.id, { kind: 'duplicate', groupSize: group.rows.length, sameStart: group.sameStart })
      if (group.sameStart) mergeableGroupOf.set(row.id, group)
    }
    if (!group.sameStart) conflicts++
  }

  const day = framesPerDay(show.fps)
  const timed: TimedRow[] = []
  for (const row of show.rows) {
    if (row.absent) {
      flag(row.id, { kind: 'absent' })
      warnings++
    }
    if (row.startTimecode === null) {
      flag(row.id, { kind: 'blankStart' })
      warnings++
      continue
    }
    const start = parseTimecode(row.startTimecode, show.fps)
    if (start === null) {
      flag(row.id, { kind: 'invalidFrames' })
      conflicts++
      continue
    }
    if (row.durationSeconds <= 0) continue
    const end = start + row.durationSeconds * show.fps
    if (end >= day) {
      flag(row.id, { kind: 'crossesMidnight' })
      conflicts++
    }
    timed.push({ id: row.id, start, end })
  }

  for (const [a, b] of overlappingPairs(timed, day)) {
    // Copies of one track at one start overlap by definition; merging them is the fix.
    const group = mergeableGroupOf.get(a)
    if (group !== undefined && group === mergeableGroupOf.get(b)) continue
    flag(a, { kind: 'overlap', withRowId: b })
    flag(b, { kind: 'overlap', withRowId: a })
    conflicts++
  }

  return { byRow, conflicts, warnings, ready: conflicts === 0 }
}

/** What switching `show` to `newFps` does to its starts, for the confirmation. */
export function fpsChangeImpact(
  show: ShowData,
  newFps: ShowFps
): { rowsWithStart: number; invalidRowIds: string[] } {
  const withStart = show.rows.filter((row) => row.startTimecode !== null)
  return {
    rowsWithStart: withStart.length,
    invalidRowIds: withStart
      .filter(
        (row) => row.startTimecode !== null && parseTimecode(row.startTimecode, newFps) === null
      )
      .map((row) => row.id)
  }
}

/**
 * Drops the extra copies of a track listed several times at one start,
 * keeping the first copy with its trim. Duplicates with different or blank
 * starts are left for the operator to settle.
 */
export function mergeSameStartDuplicates(rows: readonly ShowRow[]): ShowRow[] {
  const dropped = new Set<string>()
  for (const group of duplicateGroups(rows)) {
    if (!group.sameStart) continue
    for (const extra of group.rows.slice(1)) dropped.add(extra.id)
  }
  return rows.filter((row) => !dropped.has(row.id))
}

/**
 * Numbers every blank row at the previous numbered row's start plus
 * `stepFrames`, the first one at `00:00:00:00` when no numbered row precedes
 * it. Typed starts are left alone, and a start that does not parse at `fps`
 * is neither changed nor stepped from. The overlaps this may create are
 * {@link analyseShow}'s to report.
 */
export function fillBlanks(rows: readonly ShowRow[], stepFrames: number, fps: ShowFps): ShowRow[] {
  let previousStart: number | null = null
  return rows.map((row) => {
    if (row.startTimecode !== null) {
      previousStart = parseTimecode(row.startTimecode, fps) ?? previousStart
      return row
    }
    const start: number = previousStart === null ? 0 : previousStart + stepFrames
    previousStart = start % framesPerDay(fps)
    return { ...row, startTimecode: formatTimecode(start, fps) }
  })
}
