import { describe, expect, it } from 'vitest'

import {
  isNewerVersion,
  parseLatestRelease,
  updateToAnnounce,
  versionFromTag
} from './update-check'

describe('versionFromTag', () => {
  it('strips the leading v of a release tag', () => {
    expect(versionFromTag('v1.6.0')).toBe('1.6.0')
    expect(versionFromTag('1.6.0')).toBe('1.6.0')
  })

  it('rejects tags that are not a plain release version', () => {
    expect(versionFromTag('build-98becc7')).toBeNull()
    expect(versionFromTag('v1.6')).toBeNull()
    expect(versionFromTag('v1.6.0-beta.1')).toBeNull()
  })
})

describe('isNewerVersion', () => {
  it('compares each part numerically, not as text', () => {
    expect(isNewerVersion('1.10.0', '1.9.0')).toBe(true)
    expect(isNewerVersion('2.0.0', '1.99.99')).toBe(true)
    expect(isNewerVersion('1.5.1', '1.5.0')).toBe(true)
  })

  it('is false for the same or an older version', () => {
    expect(isNewerVersion('1.5.0', '1.5.0')).toBe(false)
    expect(isNewerVersion('1.4.9', '1.5.0')).toBe(false)
  })

  it('is false when either version cannot be read', () => {
    expect(isNewerVersion('latest', '1.5.0')).toBe(false)
    expect(isNewerVersion('1.6.0', 'dev')).toBe(false)
  })
})

describe('parseLatestRelease', () => {
  it('reads the version from the tag name', () => {
    expect(parseLatestRelease({ tag_name: 'v1.6.0', html_url: 'https://example.com' })).toBe(
      '1.6.0'
    )
  })

  it('rejects answers without a usable tag', () => {
    expect(parseLatestRelease(null)).toBeNull()
    expect(parseLatestRelease({ message: 'Not Found' })).toBeNull()
    expect(parseLatestRelease({ tag_name: 160 })).toBeNull()
    expect(parseLatestRelease({ tag_name: 'build-98becc7' })).toBeNull()
  })
})

describe('updateToAnnounce', () => {
  it('announces a newer release', () => {
    expect(updateToAnnounce('1.6.0', '1.5.0', null)).toBe('1.6.0')
  })

  it('stays quiet while either version is unknown', () => {
    expect(updateToAnnounce(null, '1.5.0', null)).toBeNull()
    expect(updateToAnnounce('1.6.0', null, null)).toBeNull()
  })

  it('stays quiet when the app is up to date', () => {
    expect(updateToAnnounce('1.5.0', '1.5.0', null)).toBeNull()
  })

  it('stays quiet for the version the user dismissed, but not for a later one', () => {
    expect(updateToAnnounce('1.6.0', '1.5.0', '1.6.0')).toBeNull()
    expect(updateToAnnounce('1.7.0', '1.5.0', '1.6.0')).toBe('1.7.0')
  })
})
