/**
 * What importing a playlist will do to the show, worked out before anything
 * changes so the Merge / Replace dialog can say it in numbers.
 */

import type { ShowRow } from './show'
import { mergeRows, replaceRows, type ImportedRow } from './show-merge'

export type ImportMode = 'merge' | 'replace'

export interface ImportPlan {
  /** The show's rows once the import is applied. */
  rows: ShowRow[]
  /** The show already has rows, so the operator chooses Merge or Replace first. */
  needsConfirmation: boolean
  /** Existing rows paired with a playlist track, start and trim kept. */
  kept: number
  /** Playlist tracks that become new blank rows. */
  added: number
  /** Existing rows the playlist does not list, kept and marked absent. */
  absent: number
  /** Existing rows that disappear. */
  dropped: number
}

/** The outcome of importing `imported` into a show holding `existing`, by `mode`. */
export function planImport(
  existing: readonly ShowRow[],
  imported: readonly ImportedRow[],
  mode: ImportMode
): ImportPlan {
  const needsConfirmation = existing.length > 0
  if (mode === 'replace') {
    return {
      rows: replaceRows(imported),
      needsConfirmation,
      kept: 0,
      added: imported.length,
      absent: 0,
      dropped: existing.length
    }
  }

  const existingIds = new Set(existing.map((row) => row.id))
  const rows = mergeRows(existing, imported)
  const fromExisting = rows.filter((row) => existingIds.has(row.id))
  return {
    rows,
    needsConfirmation,
    kept: fromExisting.filter((row) => !row.absent).length,
    added: rows.length - fromExisting.length,
    absent: fromExisting.filter((row) => row.absent).length,
    dropped: 0
  }
}
