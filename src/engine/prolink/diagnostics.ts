import { MediaSlot, TrackType } from 'prolink-connect'

/**
 * Structured trace of everything that happens on the metadata path.
 *
 * Metadata lookups are the one part of Pro DJ Link that fails silently:
 * prolink-connect returns `null` when it has no strategy for a track, and
 * several of its internal waits have no timeout at all, so a player that
 * doesn't answer looks exactly like a player with no track loaded. These
 * traces are what makes the difference visible from a booth, after the fact.
 *
 * Events are rare (a handful per track load), so they are always recorded —
 * no debug flag to remember to turn on before the set.
 */

type Sink = (line: string) => void

let sink: Sink | null = null

/**
 * Installs a persistent destination for the trace (a log file, set up by the
 * main process). The engine layer stays free of Electron this way.
 */
export function setDiagnosticSink(fn: Sink | null): void {
  sink = fn
}

function formatValue(value: unknown): string {
  if (value === null || value === undefined) return '-'
  if (value instanceof Error) return `${value.name}: ${value.message}`
  if (typeof value === 'string') return value.includes(' ') ? JSON.stringify(value) : value
  return String(value)
}

/** Records one event, e.g. `diag('track:local-failed', { deck: 2, error })`. */
export function diag(event: string, data: Record<string, unknown> = {}): void {
  const fields = Object.entries(data)
    .map(([key, value]) => `${key}=${formatValue(value)}`)
    .join(' ')
  const line = `${event}${fields ? ` ${fields}` : ''}`
  console.log(`[prolink] ${line}`)
  sink?.(`${new Date().toISOString()}\t${line}`)
}

/** `3` → `USB`, so slot numbers in the trace can be read at a glance. */
export function slotName(slot: number): string {
  return MediaSlot[slot] ?? `unknown(${slot})`
}

/** `1` → `RB` (a Rekordbox-analyzed track), `2` → `Unanalyzed`, … */
export function trackTypeName(type: number): string {
  return TrackType[type] ?? `unknown(${type})`
}

export class TimeoutError extends Error {
  constructor(label: string, ms: number) {
    super(`${label} timed out after ${ms}ms`)
    this.name = 'TimeoutError'
  }
}

/**
 * Fails a promise that takes too long, and reports it if it settles later.
 *
 * Several prolink-connect waits are unbounded — the media-slot query resolves
 * on `once('mediaSlot')` with no timer, so an unanswered query never returns.
 * Rejecting cannot cancel the underlying work, but it hands control back to
 * us, which is what lets a fallback run instead of the deck staying blank.
 */
export function withTimeout<T>(promise: Promise<T>, ms: number, label: string): Promise<T> {
  let timer: NodeJS.Timeout
  let timedOut = false

  const timeout = new Promise<never>((_resolve, reject) => {
    timer = setTimeout(() => {
      timedOut = true
      reject(new TimeoutError(label, ms))
    }, ms)
  })

  promise.then(
    () => timedOut && diag('timeout:late-success', { label, afterMs: ms }),
    (error) => timedOut && diag('timeout:late-failure', { label, error })
  )

  return Promise.race([promise, timeout]).finally(() => clearTimeout(timer))
}
