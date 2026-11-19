/**
 * What a show row looks like in the editor: its trim and duration as text,
 * the trim stepper's arithmetic, and its issues as ordered badges — kept out
 * of the components so they stay thin and this stays testable.
 */

import type { RowIssue } from './show-validation'
import { MAX_TRIM_MS, trimFramesToMs, trimMsToFrames, type ShowFps } from './show-timecode'

const SECONDS_PER_MINUTE = 60
const MINUS_SIGN = '−'

/** The trim in signed whole frames, e.g. "+2f", "−1f", "0f". */
export function trimLabel(trimMs: number, fps: ShowFps): string {
  const frames = trimMsToFrames(trimMs, fps)
  if (frames > 0) return `+${frames}f`
  if (frames < 0) return `${MINUS_SIGN}${-frames}f`
  return '0f'
}

/** The trim one frame further `direction`, in ms, kept within ±{@link MAX_TRIM_MS}. */
export function stepTrim(trimMs: number, fps: ShowFps, direction: 1 | -1): number {
  const maxFrames = trimMsToFrames(MAX_TRIM_MS, fps)
  const frames = trimMsToFrames(trimMs, fps) + direction
  return trimFramesToMs(Math.max(-maxFrames, Math.min(maxFrames, frames)), fps)
}

/** A whole-second duration as "M:SS", or "unknown" — an unknown duration never matches a deck. */
export function durationLabel(durationSeconds: number): string {
  if (durationSeconds <= 0) return 'unknown'
  const minutes = Math.floor(durationSeconds / SECONDS_PER_MINUTE)
  const seconds = durationSeconds % SECONDS_PER_MINUTE
  return `${minutes}:${String(seconds).padStart(2, '0')}`
}

/** Conflicts keep the show from being ready; warnings do not. */
export type BadgeTone = 'conflict' | 'warning'

export interface RowBadge {
  kind: RowIssue['kind']
  label: string
  tone: BadgeTone
}

/** Badge order on a row: what the row is, then what it collides with, then what it lacks. */
const BADGE_ORDER: readonly RowIssue['kind'][] = [
  'absent',
  'overlap',
  'crossesMidnight',
  'duplicate',
  'invalidFrames',
  'blankStart'
]

function badgeOf(issue: RowIssue, overlaps: number): RowBadge {
  switch (issue.kind) {
    case 'absent':
      return { kind: issue.kind, label: 'Not in playlist', tone: 'warning' }
    case 'overlap':
      return {
        kind: issue.kind,
        label: overlaps > 1 ? `Overlaps ${overlaps} rows` : 'Overlaps',
        tone: 'conflict'
      }
    case 'crossesMidnight':
      return { kind: issue.kind, label: 'Crosses midnight', tone: 'conflict' }
    case 'duplicate':
      return issue.sameStart
        ? { kind: issue.kind, label: `Listed ${issue.groupSize}×, same start`, tone: 'warning' }
        : { kind: issue.kind, label: `Listed ${issue.groupSize}×`, tone: 'conflict' }
    case 'invalidFrames':
      return { kind: issue.kind, label: 'Frames too high', tone: 'conflict' }
    case 'blankStart':
      return { kind: issue.kind, label: 'No start', tone: 'warning' }
  }
}

/** One badge per kind of issue on a row, in {@link BADGE_ORDER}. */
export function rowBadges(issues: readonly RowIssue[]): RowBadge[] {
  const overlaps = issues.filter((issue) => issue.kind === 'overlap').length
  return BADGE_ORDER.flatMap((kind) => {
    const issue = issues.find((candidate) => candidate.kind === kind)
    return issue === undefined ? [] : [badgeOf(issue, overlaps)]
  })
}

/** Whether any of a row's badges keeps the show from being ready. */
export function rowInConflict(badges: readonly RowBadge[]): boolean {
  return badges.some((badge) => badge.tone === 'conflict')
}
