import { describe, expect, it } from 'vitest'

import { countLabel } from './count-label'

describe('countLabel', () => {
  it('agrees the noun with the count', () => {
    expect(countLabel(0, 'track')).toBe('0 tracks')
    expect(countLabel(1, 'track')).toBe('1 track')
    expect(countLabel(2, 'entry', 'entries')).toBe('2 entries')
  })
})
