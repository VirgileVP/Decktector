import { invoke, isTauri } from '@tauri-apps/api/core'
import type { StoreApi } from 'zustand'

import type { ShowData } from '../lib/show'
import { FALLBACK_START_TC_DEFAULT, frameRateOfFps, parseTimecode } from '../lib/show-timecode'
import type { TimecodeShow } from '../model'
import { useShow, type ShowStore } from './show'

/**
 * The show as the timecode service takes it — the one place start-timecode
 * labels become frame counts, at the show's own fps. A label whose frames do
 * not fit that fps is pushed blank, so its track falls back (or is cut)
 * instead of running from a converted start.
 */
export function toTimecodeShow(show: ShowData, revision: number): TimecodeShow {
  return {
    revision,
    frameRate: frameRateOfFps(show.fps),
    fallbackStartFrames:
      parseTimecode(show.fallbackStartTimecode, show.fps) ??
      parseTimecode(FALLBACK_START_TC_DEFAULT, show.fps) ??
      0,
    cutWhenUnidentified: show.cutWhenUnidentified,
    rows: show.rows.map(({ id, title, artist, durationSeconds, startTimecode, trimMs }) => ({
      id,
      title,
      artist,
      durationSeconds,
      startFrames: startTimecode === null ? null : parseTimecode(startTimecode, show.fps),
      trimMs
    }))
  }
}

/**
 * Hands out increasing revisions. Seeded from the clock rather than 0, so a
 * reloaded renderer never pushes a revision the service already holds for
 * another table — it would take the new table for the one it has.
 */
export function revisionCounter(now: () => number = Date.now): () => number {
  let last = 0
  return () => {
    last = Math.max(last + 1, now())
    return last
  }
}

const nextRevision = revisionCounter()

/** Sends `show` to the timecode service; null unloads it. Resolves once it is applied (or failed). */
export function pushTimecodeShow(show: ShowData | null): Promise<void> {
  return invoke('set_timecode_show', {
    show: show === null ? null : toTimecodeShow(show, nextRevision())
  }).then(
    () => undefined,
    (error) => console.error('Failed to apply the show to the timecode service:', error)
  )
}

/**
 * Runs the startup pushes in order: the show first, then the protocol
 * config. The service starts with its output off, so enabling it only once
 * the table is there means it never sends `00:00:00:00` for a track that has
 * a row.
 */
export async function pushShowThenConfig(
  pushShow: () => Promise<void>,
  pushConfig: () => void
): Promise<void> {
  await pushShow()
  pushConfig()
}

/**
 * Calls `push` with every new show `store` holds. The startup push is
 * `pushShowThenConfig`'s, so only changes go through here.
 */
export function subscribeShowPushes(
  store: Pick<StoreApi<ShowStore>, 'subscribe'>,
  push: (show: ShowData | null) => void
): () => void {
  return store.subscribe((state, previous) => {
    if (state.show !== previous.show) push(state.show)
  })
}

/**
 * Pushes the show to the timecode service whenever it changes. Main window
 * only, like the timecode config: a panel window mirrors the show, never
 * pushes it. Returns the teardown function.
 */
export function connectShowToEngine(): () => void {
  if (!isTauri()) return () => {}
  return subscribeShowPushes(useShow, (show) => void pushTimecodeShow(show))
}
