const MS_PER_SECOND = 1000
const SECONDS_PER_MINUTE = 60
const MS_PER_TENTH = 100
const TENTHS_PER_SECOND = 10

/** `mm:ss.t` like a CDJ's time readout; negative input is clamped to zero. */
export function formatTimeTenths(ms: number): string {
  const totalTenths = Math.floor(Math.max(0, ms) / MS_PER_TENTH)
  const totalSeconds = Math.floor(totalTenths / TENTHS_PER_SECOND)
  const minutes = Math.floor(totalSeconds / SECONDS_PER_MINUTE)
  const seconds = totalSeconds % SECONDS_PER_MINUTE
  const tenths = totalTenths % TENTHS_PER_SECOND
  return `${pad2(minutes)}:${pad2(seconds)}.${tenths}`
}

/** `m:ss` for the library list. */
export function formatDuration(ms: number): string {
  const totalSeconds = Math.round(Math.max(0, ms) / MS_PER_SECOND)
  const minutes = Math.floor(totalSeconds / SECONDS_PER_MINUTE)
  return `${minutes}:${pad2(totalSeconds % SECONDS_PER_MINUTE)}`
}

function pad2(value: number): string {
  return value.toString().padStart(2, '0')
}
