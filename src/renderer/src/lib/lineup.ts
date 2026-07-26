/**
 * The night's running order, as wall-clock minutes since midnight.
 *
 * No absolute dates: a set is a start time and a name, and it ends when the next
 * one starts. That keeps the model free of timezones, DST and any notion of
 * "which night is it", and lets the same lineup be reused next weekend. The
 * night's own end is stored apart, since nothing follows it.
 *
 * Times before `NIGHT_ROLLOVER_MINUTES` are treated as belonging to the previous
 * evening, so 01:00 sorts after 23:30 without the user flagging it. The
 * consequence is that a lineup crossing 06:00 (an after-hours running until 08:00)
 * cannot be expressed; the constant is exported so it can become a setting later.
 */

export interface LineupSlot {
  /** Stable across edits so the editor can reorder rows without losing focus. */
  id: string
  artist: string
  /** Wall-clock minutes since midnight, or null while the row is being filled. */
  startMinutes: number | null
}

/** A slot placed on the timeline, with the end its successor gives it. */
export interface ResolvedSlot {
  id: string
  artist: string
  startMinutes: number
  /** Start of the following set, the night's end, or null if neither is set. */
  endMinutes: number | null
}

/** A resolved slot read against the current time, ready to render. */
export interface LineupEntry extends ResolvedSlot {
  state: 'passed' | 'current' | 'upcoming'
}

export const MINUTES_PER_DAY = 1440
export const MINUTES_PER_HOUR = 60

/** Anything earlier than this belongs to the night that started the day before. */
export const NIGHT_ROLLOVER_MINUTES = 6 * MINUTES_PER_HOUR

/** Monotonic sort key over a night, derived from a single time. */
export function nightOrder(minutes: number): number {
  return minutes < NIGHT_ROLLOVER_MINUTES ? minutes + MINUTES_PER_DAY : minutes
}

export function minutesOfDay(date: Date): number {
  return date.getHours() * MINUTES_PER_HOUR + date.getMinutes()
}

export function createSlot(): LineupSlot {
  return { id: crypto.randomUUID(), artist: '', startMinutes: null }
}

export function formatClockMinutes(minutes: number): string {
  const wrapped = ((minutes % MINUTES_PER_DAY) + MINUTES_PER_DAY) % MINUTES_PER_DAY
  const hours = Math.floor(wrapped / MINUTES_PER_HOUR)
  return `${String(hours).padStart(2, '0')}:${String(wrapped % MINUTES_PER_HOUR).padStart(2, '0')}`
}

/**
 * Reads back an `<input type="time">` value. The element only ever yields
 * `HH:MM` or an empty string, but this is where a DOM value enters the model,
 * so it is checked rather than trusted.
 */
export function parseTimeInput(value: string): number | null {
  const match = /^(\d{2}):(\d{2})$/.exec(value)
  if (!match) return null

  const hours = Number(match[1])
  const minutes = Number(match[2])
  if (hours > 23 || minutes > 59) return null

  return hours * MINUTES_PER_HOUR + minutes
}

/** The `HH:MM` an `<input type="time">` expects; empty clears the field. */
export function toTimeInput(minutes: number | null): string {
  return minutes === null ? '' : formatClockMinutes(minutes)
}

/** Compact span, e.g. `1h30` or `45min`. */
export function formatDuration(minutes: number): string {
  const hours = Math.floor(minutes / MINUTES_PER_HOUR)
  const rest = minutes % MINUTES_PER_HOUR
  if (hours === 0) return `${rest}min`
  return rest === 0 ? `${hours}h` : `${hours}h${String(rest).padStart(2, '0')}`
}

/** Chronological order over the night; rows with no time yet stay at the end. */
export function sortSlots(slots: LineupSlot[]): LineupSlot[] {
  return [...slots].sort((a, b) => {
    if (a.startMinutes === null) return b.startMinutes === null ? 0 : 1
    if (b.startMinutes === null) return -1
    return nightOrder(a.startMinutes) - nightOrder(b.startMinutes)
  })
}

/**
 * Places the timed slots on the night's timeline, each one ending where the next
 * begins. Rows still missing a time have no place on it and drop out.
 */
export function resolveLineup(slots: LineupSlot[], endMinutes: number | null): ResolvedSlot[] {
  const timed = sortSlots(slots).filter(
    (slot): slot is LineupSlot & { startMinutes: number } => slot.startMinutes !== null
  )

  return timed.map((slot, index) => {
    const next = timed[index + 1]
    return {
      id: slot.id,
      artist: slot.artist,
      startMinutes: slot.startMinutes,
      endMinutes: next === undefined ? endMinutes : next.startMinutes
    }
  })
}

/** Minutes a resolved set runs for, or null while its end is unknown. */
export function slotDuration(slot: ResolvedSlot): number | null {
  if (slot.endMinutes === null) return null
  return nightOrder(slot.endMinutes) - nightOrder(slot.startMinutes)
}

/**
 * Reads the lineup against the current time. Unnamed rows shape the timeline
 * while they are being filled in, but never reach the display.
 */
export function buildLineup(
  slots: LineupSlot[],
  endMinutes: number | null,
  nowMinutes: number
): LineupEntry[] {
  const now = nightOrder(nowMinutes)

  return resolveLineup(slots, endMinutes)
    .filter((slot) => slot.artist.trim().length > 0)
    .map((slot) => {
      const start = nightOrder(slot.startMinutes)
      const end = slot.endMinutes === null ? null : nightOrder(slot.endMinutes)
      return {
        ...slot,
        state: now < start ? 'upcoming' : end !== null && now >= end ? 'passed' : 'current'
      }
    })
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
