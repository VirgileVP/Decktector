/**
 * Clock and axis primitives shared by the lineup domain and the timeline
 * scale. Kept ignorant of slots: nothing here knows what a set is.
 */

export const MINUTES_PER_DAY = 1440
export const MINUTES_PER_HOUR = 60

/** Two days of minutes, the show's whole addressable axis. */
export const AXIS_MINUTES = 2 * MINUTES_PER_DAY

/** Minutes since midnight of the show's first day, 0..2879. */
export type AxisMinutes = number

export function minutesOfDay(date: Date): number {
  return date.getHours() * MINUTES_PER_HOUR + date.getMinutes()
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

/** Clamps to the axis's addressable range, 0..AXIS_MINUTES. */
export function clampToAxis(minutes: number): AxisMinutes {
  return Math.min(AXIS_MINUTES, Math.max(0, minutes))
}

/** 0 for the show's first day, 1 for the day after. */
export function axisDay(minutes: AxisMinutes): 0 | 1 {
  return minutes < MINUTES_PER_DAY ? 0 : 1
}

/** The clock reading an axis position corresponds to, 0..1439. */
export function axisClock(minutes: AxisMinutes): number {
  return minutes % MINUTES_PER_DAY
}

/** Places a clock reading on a given day of the axis. */
export function axisFromClock(clockMinutes: number, day: 0 | 1): AxisMinutes {
  return day * MINUTES_PER_DAY + clockMinutes
}

/** Flips a position between the show's two days, keeping its clock reading. */
export function toggleAxisDay(minutes: AxisMinutes): AxisMinutes {
  const nextDay = axisDay(minutes) === 0 ? 1 : 0
  return axisFromClock(axisClock(minutes), nextDay)
}

/**
 * The first axis position reading `clockMinutes` that does not precede
 * `afterMinutes`. With no predecessor, anchors on the show's first day.
 */
export function axisAfter(clockMinutes: number, afterMinutes: AxisMinutes | null): AxisMinutes {
  if (afterMinutes === null) return axisFromClock(clockMinutes, 0)

  const sameDay = axisFromClock(clockMinutes, 0)
  return sameDay >= afterMinutes ? sameDay : axisFromClock(clockMinutes, 1)
}

/** `22:30` or `02:00 +1`, for the rail's labels. */
export function formatAxisLabel(minutes: AxisMinutes): string {
  const label = formatClockMinutes(axisClock(minutes))
  return axisDay(minutes) === 1 ? `${label} +1` : label
}
