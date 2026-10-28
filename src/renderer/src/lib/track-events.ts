import type { TrackEvent, TrackEventKind } from '../model'

/** How each section reads in the UI. */
export const EVENT_LABELS: Record<TrackEventKind, string> = {
  intro: 'Intro',
  build: 'Build',
  drop: 'Drop',
  break: 'Break',
  groove: 'Groove',
  outro: 'Outro'
}

/**
 * Below this the label is a guess, and the row says so rather than claiming a
 * drop that may not come. Matches the confidence a section reaches on energy
 * evidence alone, without a cue confirming it.
 */
export const EVENT_CONFIDENCE_HEDGE = 0.7

/**
 * The section's name: rekordbox's own for a phrase it analysed ("Verse 2"),
 * otherwise the name of the kind it was inferred to be.
 */
export function eventName(event: TrackEvent): string {
  return event.label ?? EVENT_LABELS[event.kind]
}

/** Where the playhead sits inside the track's event list. */
export interface EventProgress {
  /** Index in the event list of the section being counted down. */
  index: number
  /** Milliseconds until that section ends. */
  remainingMs: number
  /** How far through the section the playhead is, 0-1. */
  ratio: number
}

/**
 * Locates the playhead in the event list.
 *
 * Sections are contiguous, so this normally finds the one the playhead is in.
 * A playhead before the first section — or in a gap, which only a partial
 * analysis produces — counts down the next section from its full length, so
 * the UI always names something ahead rather than blanking out.
 */
export function eventProgressAt(events: TrackEvent[], playheadMs: number): EventProgress | null {
  const index = events.findIndex((event) => playheadMs < event.endMs)
  if (index === -1) return null

  const event = events[index]
  const length = event.endMs - event.startMs
  if (length <= 0) return null

  const elapsed = Math.max(0, playheadMs - event.startMs)
  return {
    index,
    remainingMs: event.endMs - Math.max(playheadMs, event.startMs),
    ratio: Math.min(1, elapsed / length)
  }
}

/** Whole seconds, floored, as the countdown and the durations both show them. */
export function formatEventSeconds(ms: number): string {
  return `${Math.max(0, Math.floor(ms / 1000))}s`
}
