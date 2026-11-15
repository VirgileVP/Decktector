import { describe, expect, it } from 'vitest'

import { CUSTOM_LOGO_MAX_BYTES, isAcceptedLogoType, parseCustomLogo } from './custom-logo'

const PNG_DATA_URL = 'data:image/png;base64,iVBORw0KGgo='

describe('parseCustomLogo', () => {
  it('keeps a base64 data URL of an accepted image type', () => {
    expect(parseCustomLogo(PNG_DATA_URL)).toBe(PNG_DATA_URL)
    expect(parseCustomLogo('data:image/svg+xml;base64,PHN2Zy8+')).toBe(
      'data:image/svg+xml;base64,PHN2Zy8+'
    )
  })

  it('falls back to the built-in logo for anything else', () => {
    for (const raw of [
      null,
      undefined,
      42,
      '',
      'https://example.com/logo.png',
      'data:text/html;base64,PGgxPg==',
      'data:image/png,not-base64',
      'data:image/png;base64,abc"onerror="x'
    ]) {
      expect(parseCustomLogo(raw)).toBeNull()
    }
  })

  it('rejects a logo larger than the size cap', () => {
    const oversized = `data:image/png;base64,${'A'.repeat(CUSTOM_LOGO_MAX_BYTES * 2)}`
    expect(parseCustomLogo(oversized)).toBeNull()
  })
})

describe('isAcceptedLogoType', () => {
  it('accepts common image types only', () => {
    expect(isAcceptedLogoType('image/png')).toBe(true)
    expect(isAcceptedLogoType('image/jpeg')).toBe(true)
    expect(isAcceptedLogoType('image/bmp')).toBe(false)
    expect(isAcceptedLogoType('application/pdf')).toBe(false)
  })
})
