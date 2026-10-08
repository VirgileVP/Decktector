/**
 * Upgrades what an older build left in the panel windows store. Kept apart
 * from `panel-windows.ts`, which reaches into Tauri at import time, so it
 * can be tested on its own.
 */

import { isRecord } from '../lib/is-record'

/** Bumped whenever a stored record changes shape; see `migratePanelWindowsState`. */
export const PANEL_WINDOWS_STORE_VERSION = 1

/**
 * Version 0 records carried a single `compact` flag where version 1 has a
 * `display`. A compact window becomes a compact-layout display and is left
 * partial on purpose: the window fills the rest in when it reads it back
 * (`resolvePanelDisplay`), exactly as for any other incomplete record.
 */
function migrateRecordFromVersion0(record: unknown): unknown {
  if (!isRecord(record) || !('compact' in record)) return record
  const { compact, ...rest } = record
  return compact === true ? { ...rest, display: { layout: 'compact' } } : rest
}

/** zustand `persist`'s `migrate`: brings `persisted`, stored at `version`, up to the current shape. */
export function migratePanelWindowsState(persisted: unknown, version: number): unknown {
  if (version >= PANEL_WINDOWS_STORE_VERSION) return persisted
  if (!isRecord(persisted) || !isRecord(persisted.windows)) return persisted

  const windows = Object.fromEntries(
    Object.entries(persisted.windows).map(([label, record]) => [
      label,
      migrateRecordFromVersion0(record)
    ])
  )
  return { ...persisted, windows }
}
