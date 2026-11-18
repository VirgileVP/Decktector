/**
 * What importing a playlist does to the show's rows: **Replace** starts over,
 * **Merge** keeps the work already done on the tracks the playlist still has.
 *
 * Merge never deletes: a row the playlist no longer lists is kept and marked
 * absent, so a start timecode the operator typed survives a playlist edit
 * that dropped its track by mistake.
 */

import { createRow, type ShowRow } from './show'
import { identityKey, identityMatches, type TrackIdentity } from './show-identity'

/** A track as a playlist import supplies it. */
export type ImportedRow = TrackIdentity

/** A new blank row for `track`, taking only the fields a row keeps. */
function rowFor({ title, artist, durationSeconds }: ImportedRow): ShowRow {
  return createRow({ title, artist, durationSeconds })
}

/** Fresh rows for every imported track, in playlist order: blank starts, no trim, new ids. */
export function replaceRows(imported: readonly ImportedRow[]): ShowRow[] {
  return imported.map(rowFor)
}

/**
 * The playlist's tracks in its order, each paired with the first existing row
 * it matches that no earlier track took — so a track listed twice pairs with
 * two rows. A paired row is kept as it is (id, start, trim, set artist, and
 * its own title, artist and duration text) and is no longer absent; an
 * unpaired track becomes a new blank row. The rows no track took follow,
 * marked absent, in their previous order.
 */
export function mergeRows(
  existing: readonly ShowRow[],
  imported: readonly ImportedRow[]
): ShowRow[] {
  const candidatesByKey = new Map<string, ShowRow[]>()
  for (const row of existing) {
    const key = identityKey(row)
    const candidates = candidatesByKey.get(key)
    if (candidates === undefined) candidatesByKey.set(key, [row])
    else candidates.push(row)
  }

  const paired = new Set<string>()
  const merged = imported.map((track): ShowRow => {
    const match = candidatesByKey
      .get(identityKey(track))
      ?.find((row) => !paired.has(row.id) && identityMatches(row, track))
    if (match === undefined) return rowFor(track)
    paired.add(match.id)
    return match.absent ? { ...match, absent: false } : match
  })

  const left = existing
    .filter((row) => !paired.has(row.id))
    .map((row) => (row.absent ? row : { ...row, absent: true }))
  return [...merged, ...left]
}
