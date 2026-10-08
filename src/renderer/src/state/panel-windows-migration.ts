/**
 * Upgrades what an older build left in the panel windows store. Kept apart
 * from `panel-windows.ts`, which reaches into Tauri at import time, so it
 * can be tested on its own.
 */

import { isRecord } from '../lib/is-record'
import { DECK_VIEW_PRESETS } from './settings'

/** Bumped whenever a stored record changes shape; see `migratePanelWindowsState`. */
export const PANEL_WINDOWS_STORE_VERSION = 1

/**
 * Version 0 records carried a single `compact` flag where version 1 has a
 * `display`. The compact layout is gone: a compact window becomes the
 * Simple view preset, its closest match; any other one gets no display and
 * starts from the main window's toggles (`resolvePanelDisplay`).
 */
function migrateRecordFromVersion0(record: unknown): unknown {
  if (!isRecord(record) || !('compact' in record)) return record
  const { compact, ...rest } = record
  return compact === true ? { ...rest, display: { ...DECK_VIEW_PRESETS.simple.values } } : rest
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
