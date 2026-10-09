/**
 * The night's running order, placed on a two-day axis (see `lib/lineup-axis`).
 *
 * A set is a start position and a name; it ends where the next one starts, or
 * where the night itself ends if it is the last. That keeps the model free of
 * durations to keep in sync, at the cost of one consequence: moving a block's
 * body stretches or shrinks its predecessor, since the rail has no gaps.
 */

import { AXIS_MINUTES, axisFromClock, clampToAxis, type AxisMinutes } from './lineup-axis'

export interface LineupSlot {
  /** Stable across edits so the editor can reorder rows without losing focus. */
  id: string
  artist: string
  /** Free text for the booth, shown only in the "Time" panel window; empty when unused. */
  note: string
  /** Axis position, or null while the row is being filled in. */
  startOffset: AxisMinutes | null
}

/** A whole night's running order: its sets and the two markers around them. */
export interface LineupData {
  slots: LineupSlot[]
  /** When the last set stops; null leaves the night open-ended. */
  endOffset: AxisMinutes | null
  /** When doors open, ahead of the first set; null hides the marker. */
  doorsOffset: AxisMinutes | null
}

/** A slot placed on the timeline, with the end its successor gives it. */
export interface ResolvedSlot {
  id: string
  artist: string
  /** The set's note; always empty on the doors and END SHOW markers. */
  note: string
  startOffset: AxisMinutes
  /** Start of the following set, the night's end, or null if neither is set. */
  endOffset: AxisMinutes | null
}

/** A resolved slot read against the current time, ready to render. */
export interface LineupEntry extends ResolvedSlot {
  state: 'passed' | 'current' | 'upcoming'
  /** Share of the set already played, from 0 to 1 — null unless it is the
   *  current one and has an end to measure against. */
  progress: number | null
}

/** The night's used range, from its first marker to its last. */
export interface LineupSpan {
  startOffset: AxisMinutes
  endOffset: AxisMinutes
}

/** The smallest a block can be dragged down to before it swallows a neighbor. */
export const MIN_BLOCK_MINUTES = 5

/** Stable ids for the two synthetic markers bracketing the night's sets. */
export const DOORS_OPEN_ID = 'doors-open'
export const END_SHOW_ID = 'end-show'

/**
 * `crypto.randomUUID` requires a secure context (HTTPS, or localhost) — the
 * remote view does not run in one: it is served over plain HTTP from a LAN
 * IP. `crypto.getRandomValues` has no such restriction, so it is the
 * fallback rather than the primary path, to keep using the browser's own
 * RNG either way.
 */
export function generateId(): string {
  if (typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID()
  }
  const bytes = crypto.getRandomValues(new Uint8Array(16))
  bytes[6] = (bytes[6] & 0x0f) | 0x40 // version 4
  bytes[8] = (bytes[8] & 0x3f) | 0x80 // variant 10
  const hex = Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('')
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`
}

export function createSlot(): LineupSlot {
  return { id: generateId(), artist: '', note: '', startOffset: null }
}

/** Chronological order over the axis; rows with no time yet stay at the end. */
export function sortSlots(slots: LineupSlot[]): LineupSlot[] {
  return [...slots].sort((a, b) => {
    if (a.startOffset === null) return b.startOffset === null ? 0 : 1
    if (b.startOffset === null) return -1
    return a.startOffset - b.startOffset
  })
}

/** Room left after the last marker when there is no night's end to split with. */
const DEFAULT_SLOT_GAP_MINUTES = 30

/**
 * Where a freshly added, unnamed set should land: between the last existing
 * marker (the last timed set, or doors if there is none yet) and the night's
 * end. Splitting the remaining gap in half means each successive "+ Add a
 * set" click still lands somewhere sensible instead of piling up on the same
 * spot. Returns null only when there is nothing at all to anchor on yet.
 */
export function nextSlotOffset(
  slots: LineupSlot[],
  endOffset: AxisMinutes | null,
  doorsOffset: AxisMinutes | null
): AxisMinutes | null {
  const timed = sortSlots(slots).filter((slot) => slot.startOffset !== null)
  const last = timed[timed.length - 1]
  const anchor = last?.startOffset ?? doorsOffset

  if (anchor === null) {
    return endOffset === null ? null : clampToAxis(endOffset - MIN_BLOCK_MINUTES)
  }
  if (endOffset === null) {
    return clampToAxis(anchor + DEFAULT_SLOT_GAP_MINUTES)
  }
  if (endOffset - anchor <= 2 * MIN_BLOCK_MINUTES) {
    return clampToAxis(endOffset - MIN_BLOCK_MINUTES)
  }

  const midpoint = Math.round((anchor + endOffset) / 2 / MIN_BLOCK_MINUTES) * MIN_BLOCK_MINUTES
  return clampToAxis(midpoint)
}

/**
 * Places the timed slots on the axis, each one ending where the next begins.
 * Rows still missing a time have no place on it and drop out.
 */
export function resolveLineup(slots: LineupSlot[], endOffset: AxisMinutes | null): ResolvedSlot[] {
  const timed = sortSlots(slots).filter(
    (slot): slot is LineupSlot & { startOffset: number } => slot.startOffset !== null
  )

  return timed.map((slot, index) => {
    const next = timed[index + 1]
    return {
      id: slot.id,
      artist: slot.artist,
      note: slot.note,
      startOffset: slot.startOffset,
      endOffset: next === undefined ? endOffset : next.startOffset
    }
  })
}

/** Minutes a resolved set runs for, or null while its end is unknown. */
export function slotDuration(slot: ResolvedSlot): number | null {
  if (slot.endOffset === null) return null
  return slot.endOffset - slot.startOffset
}

/**
 * The night's blocks in rail order — doors, each set, the night's end —
 * resolved with the end each one is given by its successor. Unnamed rows are
 * excluded by default: they shape the timeline while being filled in, but a
 * caller building the header readout must not show them.
 */
export function lineupBlocks(
  slots: LineupSlot[],
  endOffset: AxisMinutes | null,
  doorsOffset: AxisMinutes | null,
  options?: { includeUnnamed?: boolean }
): ResolvedSlot[] {
  const includeUnnamed = options?.includeUnnamed ?? false
  let sets = resolveLineup(slots, endOffset)
  if (!includeUnnamed) sets = sets.filter((slot) => slot.artist.trim().length > 0)

  const blocks: ResolvedSlot[] = []

  if (doorsOffset !== null) {
    blocks.push({
      id: DOORS_OPEN_ID,
      artist: 'Doors opening',
      note: '',
      startOffset: doorsOffset,
      endOffset: sets[0]?.startOffset ?? endOffset
    })
  }

  blocks.push(...sets)

  if (endOffset !== null) {
    blocks.push({
      id: END_SHOW_ID,
      artist: 'END SHOW',
      note: '',
      startOffset: endOffset,
      endOffset: null
    })
  }

  return blocks
}

export type BoundaryKind = 'doors' | 'set' | 'end'

/** A single draggable point on the rail: doors, a set's start, or the night's end. */
export interface Boundary {
  id: string
  kind: BoundaryKind
  artist: string
  minutes: AxisMinutes
}

/**
 * Every draggable boundary, in rail order. Unlike `lineupBlocks`, unnamed
 * slots are included — a row can be dragged into position before it is
 * named.
 */
export function lineupBoundaries(
  slots: LineupSlot[],
  endOffset: AxisMinutes | null,
  doorsOffset: AxisMinutes | null
): Boundary[] {
  const boundaries: Boundary[] = []

  if (doorsOffset !== null) {
    boundaries.push({
      id: DOORS_OPEN_ID,
      kind: 'doors',
      artist: 'Doors opening',
      minutes: doorsOffset
    })
  }

  for (const slot of sortSlots(slots)) {
    if (slot.startOffset === null) continue
    boundaries.push({ id: slot.id, kind: 'set', artist: slot.artist, minutes: slot.startOffset })
  }

  if (endOffset !== null) {
    boundaries.push({ id: END_SHOW_ID, kind: 'end', artist: 'END SHOW', minutes: endOffset })
  }

  return boundaries
}

/**
 * How far a boundary can travel before it would swallow a neighbor. Feeds
 * both the drag clamp and `aria-valuemin`/`aria-valuemax`, so keyboard and
 * pointer input can never disagree.
 */
export function boundaryLimits(
  boundaries: readonly Boundary[],
  id: string
): { min: AxisMinutes; max: AxisMinutes } {
  const index = boundaries.findIndex((boundary) => boundary.id === id)
  const previous = index > 0 ? boundaries[index - 1] : null
  const next = index >= 0 && index < boundaries.length - 1 ? boundaries[index + 1] : null

  return {
    min: clampToAxis(previous === null ? 0 : previous.minutes + MIN_BLOCK_MINUTES),
    max: clampToAxis(next === null ? AXIS_MINUTES : next.minutes - MIN_BLOCK_MINUTES)
  }
}

/**
 * The night's used range, from its earliest marker to its latest. Null when
 * nothing is placed yet — the rail and the "now" projection both fall back
 * to their empty-lineup behavior in that case.
 */
export function lineupSpan(
  slots: LineupSlot[],
  endOffset: AxisMinutes | null,
  doorsOffset: AxisMinutes | null
): LineupSpan | null {
  const boundaries = lineupBoundaries(slots, endOffset, doorsOffset)
  if (boundaries.length === 0) return null

  const minutes = boundaries.map((boundary) => boundary.minutes)
  return { startOffset: Math.min(...minutes), endOffset: Math.max(...minutes) }
}

/** How far a clock reading sits outside a span, or 0 if it is inside. */
function distanceToSpan(clockMinutes: number, span: LineupSpan): number {
  if (clockMinutes < span.startOffset) return span.startOffset - clockMinutes
  if (clockMinutes > span.endOffset) return clockMinutes - span.endOffset
  return 0
}

/**
 * Places a bare clock reading (no day, from `useClockMinutes`) on the axis,
 * given the night's used span. `useClockMinutes` returns two candidate axis
 * positions — today and tomorrow — and this picks the one the span actually
 * means:
 *
 * 1. No span (empty lineup) — the bare reading, unresolved.
 * 2. Exactly one candidate falls inside the span — that one.
 * 3. Both fall inside (a span 24h or longer) — the earlier one; a dateless
 *    model cannot tell which the user means, so the choice is made
 *    deterministically rather than arbitrarily.
 * 4. Neither falls inside — whichever is closer to the span, ties going to
 *    the earlier one.
 */
export function projectClockOnAxis(clockMinutes: number, span: LineupSpan | null): AxisMinutes {
  if (span === null) return clockMinutes

  const today = axisFromClock(clockMinutes, 0)
  const tomorrow = axisFromClock(clockMinutes, 1)
  const todayFits = today >= span.startOffset && today <= span.endOffset
  const tomorrowFits = tomorrow >= span.startOffset && tomorrow <= span.endOffset

  if (todayFits && !tomorrowFits) return today
  if (tomorrowFits && !todayFits) return tomorrow
  if (todayFits && tomorrowFits) return today

  return distanceToSpan(tomorrow, span) < distanceToSpan(today, span) ? tomorrow : today
}

/**
 * Reads the lineup against the current time. Unnamed rows shape the timeline
 * while they are being filled in, but never reach the display.
 *
 * Doors opening and END SHOW are appended as markers rather than sets: doors
 * opens first and runs until the first set (or the night's end, if there is
 * none yet), and END SHOW sits at the night's end with no close of its own, so
 * it stays the current entry once the last set is over.
 */
export function buildLineup(
  slots: LineupSlot[],
  endOffset: AxisMinutes | null,
  nowClockMinutes: number,
  doorsOffset: AxisMinutes | null = null
): LineupEntry[] {
  const span = lineupSpan(slots, endOffset, doorsOffset)
  const now = projectClockOnAxis(nowClockMinutes, span)

  return lineupBlocks(slots, endOffset, doorsOffset).map((slot) => {
    const state =
      now < slot.startOffset
        ? 'upcoming'
        : slot.endOffset !== null && now >= slot.endOffset
          ? 'passed'
          : 'current'
    return { ...slot, state, progress: state === 'current' ? setProgress(slot, now) : null }
  })
}

/** How far `now` sits into the set, or null while the set has no end. */
function setProgress(slot: ResolvedSlot, now: AxisMinutes): number | null {
  const duration = slotDuration(slot)
  if (duration === null || duration <= 0) return null
  return Math.min(Math.max((now - slot.startOffset) / duration, 0), 1)
}

/**
 * The set that just ended, the one on air and the one coming up — whichever of
 * the three exist. Before doors that is next alone, after the night nothing.
 */
export function selectAroundNow(entries: LineupEntry[]): LineupEntry[] {
  const previous = entries.filter((entry) => entry.state === 'passed').pop()
  const current = entries.find((entry) => entry.state === 'current')
  const next = entries.find((entry) => entry.state === 'upcoming')

  return [previous, current, next].filter((entry): entry is LineupEntry => entry !== undefined)
}
