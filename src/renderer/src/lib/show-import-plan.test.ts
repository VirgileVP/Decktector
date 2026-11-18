import { describe, expect, it } from 'vitest'

import type { ShowRow } from './show'
import { planImport } from './show-import-plan'
import type { ImportedRow } from './show-merge'

function existingRow(id: string, title: string, overrides: Partial<ShowRow> = {}): ShowRow {
  return {
    id,
    title,
    artist: 'Artist',
    durationSeconds: 300,
    startTimecode: '01:00:00:00',
    trimMs: 0,
    absent: false,
    ...overrides
  }
}

function imported(title: string): ImportedRow {
  return { title, artist: 'Artist', durationSeconds: 300 }
}

const EXISTING = [existingRow('a', 'A'), existingRow('b', 'B'), existingRow('c', 'C')]

describe('planImport', () => {
  it('reports kept, new and absent counts for a merge', () => {
    const plan = planImport(EXISTING, [imported('A'), imported('New'), imported('C')], 'merge')
    expect(plan).toMatchObject({
      needsConfirmation: true,
      kept: 2,
      added: 1,
      absent: 1,
      dropped: 0
    })
    expect(plan.rows.map((row) => row.title)).toEqual(['A', 'New', 'C', 'B'])
  })

  it('counts a row that was already absent and stays so', () => {
    const plan = planImport([existingRow('gone', 'Gone', { absent: true })], [], 'merge')
    expect(plan).toMatchObject({ kept: 0, added: 0, absent: 1 })
  })

  it('into an empty show needs no confirmation', () => {
    for (const mode of ['merge', 'replace'] as const) {
      const plan = planImport([], [imported('A'), imported('B')], mode)
      expect(plan.needsConfirmation).toBe(false)
      expect(plan.added).toBe(2)
      expect(plan.rows.map((row) => row.startTimecode)).toEqual([null, null])
    }
  })

  it('replace reports every existing row as dropped', () => {
    const plan = planImport(EXISTING, [imported('A')], 'replace')
    expect(plan).toMatchObject({
      needsConfirmation: true,
      kept: 0,
      added: 1,
      absent: 0,
      dropped: 3
    })
    expect(plan.rows[0].id).not.toBe('a')
    expect(plan.rows[0].startTimecode).toBeNull()
  })
})
