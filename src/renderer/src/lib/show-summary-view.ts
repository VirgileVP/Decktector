/**
 * The show editor's summary strip, as data: how many rows have each kind of
 * issue and which one comes first, plus the steps Fill blanks offers.
 */

import type { ShowRow } from './show'
import type { BadgeTone } from './show-row-view'
import type { RowIssue, ShowAnalysis } from './show-validation'
import type { ShowFps } from './show-timecode'

const SECONDS_PER_MINUTE = 60
const SECONDS_PER_HOUR = 3600

/** A Fill blanks step offered as one click. */
export interface FillStepPreset {
  label: string
  seconds: number
}

export const FILL_STEP_PRESETS: readonly FillStepPreset[] = [
  { label: '10 min', seconds: 10 * SECONDS_PER_MINUTE },
  { label: '30 min', seconds: 30 * SECONDS_PER_MINUTE },
  { label: '1 h', seconds: SECONDS_PER_HOUR }
]

/** A step of `seconds` in frames at `fps`. */
export function stepFrames(seconds: number, fps: ShowFps): number {
  return seconds * fps
}

const CUSTOM_STEP_PATTERN = /^(\d{1,2}):(\d{2}):(\d{2})$/

/**
 * A custom `HH:MM:SS` step in seconds, or null when it is malformed or zero —
 * a zero step would stack every blank row on the one before it.
 */
export function parseCustomStep(text: string): number | null {
  const match = CUSTOM_STEP_PATTERN.exec(text.trim())
  if (match === null) return null
  const [hours, minutes, seconds] = match.slice(1).map(Number)
  if (minutes >= SECONDS_PER_MINUTE || seconds >= SECONDS_PER_MINUTE) return null
  const total = hours * SECONDS_PER_HOUR + minutes * SECONDS_PER_MINUTE + seconds
  return total > 0 ? total : null
}

/** One kind of issue in the summary strip. */
export interface IssueCount {
  kind: RowIssue['kind']
  label: string
  tone: BadgeTone
  /** Rows having it. */
  rows: number
  /** The first of them in show order, to scroll to. */
  firstRowId: string
}

/** Each kind of issue as the summary reads it: conflicts first, then warnings. */
const SUMMARY_KINDS: readonly { kind: RowIssue['kind']; label: string; tone: BadgeTone }[] = [
  { kind: 'overlap', label: 'overlapping', tone: 'conflict' },
  { kind: 'crossesMidnight', label: 'past midnight', tone: 'conflict' },
  { kind: 'invalidFrames', label: 'frames too high', tone: 'conflict' },
  { kind: 'duplicate', label: 'listed twice', tone: 'conflict' },
  { kind: 'blankStart', label: 'no start', tone: 'warning' },
  { kind: 'absent', label: 'not in playlist', tone: 'warning' }
]

/** A duplicate is only a conflict when its copies do not share a start. */
function counts(issue: RowIssue, kind: RowIssue['kind']): boolean {
  if (issue.kind !== kind) return false
  return issue.kind !== 'duplicate' || !issue.sameStart
}

/** How many rows of `rows` have each kind of issue in `analysis`, kinds with none left out. */
export function summariseIssues(rows: readonly ShowRow[], analysis: ShowAnalysis): IssueCount[] {
  return SUMMARY_KINDS.flatMap(({ kind, label, tone }) => {
    const affected = rows.filter((row) =>
      (analysis.byRow.get(row.id) ?? []).some((issue) => counts(issue, kind))
    )
    return affected.length === 0
      ? []
      : [{ kind, label, tone, rows: affected.length, firstRowId: affected[0].id }]
  })
}
