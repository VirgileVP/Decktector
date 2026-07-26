import { useSyncExternalStore } from 'react'

import { minutesOfDay } from '../lib/lineup'

const MS_PER_SECOND = 1000

/**
 * One wall clock for the whole app. Readers subscribe at the granularity they
 * need, and since the snapshots are primitives React skips the re-render when
 * the value has not moved — the minute readers wake up once a minute even
 * though the timer beats every second.
 */
const listeners = new Set<() => void>()
let timer: ReturnType<typeof setTimeout> | null = null
let secondsLabel = ''
let dayMinutes = 0

function pad2(value: number): string {
  return String(value).padStart(2, '0')
}

/** Snapshots are cached rather than recomputed per call, so a render that
 *  straddles a second boundary cannot see two different clocks. */
function refresh(): void {
  const now = new Date()
  secondsLabel = `${pad2(now.getHours())}:${pad2(now.getMinutes())}:${pad2(now.getSeconds())}`
  dayMinutes = minutesOfDay(now)
}

refresh()

/** Re-arms itself on the next whole second so the display never drifts. */
function tick(): void {
  refresh()
  listeners.forEach((listener) => listener())
  timer = setTimeout(tick, MS_PER_SECOND - (Date.now() % MS_PER_SECOND))
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener)
  if (timer === null) {
    refresh()
    timer = setTimeout(tick, MS_PER_SECOND - (Date.now() % MS_PER_SECOND))
  }

  return () => {
    listeners.delete(listener)
    if (listeners.size === 0 && timer !== null) {
      clearTimeout(timer)
      timer = null
    }
  }
}

/** `HH:MM:SS`, re-rendering the caller on every second boundary. */
export function useClockSeconds(): string {
  return useSyncExternalStore(subscribe, () => secondsLabel)
}

/** Minutes since midnight, re-rendering the caller only when the minute turns. */
export function useClockMinutes(): number {
  return useSyncExternalStore(subscribe, () => dayMinutes)
}
