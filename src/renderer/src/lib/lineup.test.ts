import { afterEach, describe, expect, it } from 'vitest'

import { generateId } from './lineup'

const UUID_V4 = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/

/**
 * Restores whatever `crypto.randomUUID` was, so stubbing it out in one test
 * cannot leak into the next.
 */
const realRandomUUID = crypto.randomUUID

afterEach(() => {
  Object.defineProperty(crypto, 'randomUUID', {
    value: realRandomUUID,
    configurable: true
  })
})

describe('generateId', () => {
  it('produces a v4 UUID when crypto.randomUUID is available', () => {
    expect(generateId()).toMatch(UUID_V4)
  })

  /**
   * The remote view is served over plain HTTP from a LAN address, which is
   * not a secure context, so the browser does not expose
   * `crypto.randomUUID` there at all. Before the fallback existed, the
   * lineup's default slots threw on first render and the whole page stayed
   * blank.
   */
  it('still produces a v4 UUID when crypto.randomUUID is missing', () => {
    Object.defineProperty(crypto, 'randomUUID', { value: undefined, configurable: true })

    expect(generateId()).toMatch(UUID_V4)
  })

  it('does not repeat itself', () => {
    Object.defineProperty(crypto, 'randomUUID', { value: undefined, configurable: true })

    const ids = new Set(Array.from({ length: 500 }, () => generateId()))

    expect(ids.size).toBe(500)
  })
})
