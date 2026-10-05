import { describe, expect, it } from 'vitest'

import { canvasFont } from './canvas-font'

describe('canvasFont', () => {
  it('puts weight and size ahead of the Geist stack', () => {
    expect(canvasFont(500, 9)).toBe("500 9px 'Geist Variable', system-ui, sans-serif")
  })
})
