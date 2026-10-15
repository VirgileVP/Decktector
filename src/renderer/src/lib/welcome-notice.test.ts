import { describe, expect, it } from 'vitest'

import { shouldShowWelcomeNotice, WELCOME_NOTICE_REVISION } from './welcome-notice'

describe('shouldShowWelcomeNotice', () => {
  it('shows the notice on a first launch', () => {
    expect(shouldShowWelcomeNotice(null)).toBe(true)
  })

  it('hides the notice once the current revision was acknowledged', () => {
    expect(shouldShowWelcomeNotice(WELCOME_NOTICE_REVISION)).toBe(false)
  })

  it('shows the notice again when an older revision was acknowledged', () => {
    expect(shouldShowWelcomeNotice('0')).toBe(true)
    expect(shouldShowWelcomeNotice('')).toBe(true)
  })
})
