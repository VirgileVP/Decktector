import { describe, expect, it } from 'vitest'
import {
  extractChangelogSection,
  promoteHeadings,
  renderReleaseNotes
} from './release-notes-format.mjs'

const CHANGELOG = `# Changelog

Intro text.

## [Unreleased]

## [1.10.0] - 2026-11-01

Newer release.

## [1.1.0] - 2026-10-09

Tagline.

### Highlights

- One

### Fixes

- Two

## [1.0.0] - 2026-10-01

Older release.
`

describe('extractChangelogSection', () => {
  it('returns the section body up to the next version heading', () => {
    expect(extractChangelogSection(CHANGELOG, '1.1.0')).toBe(
      'Tagline.\n\n### Highlights\n\n- One\n\n### Fixes\n\n- Two'
    )
  })

  it('reads the last section through to the end of the file', () => {
    expect(extractChangelogSection(CHANGELOG, '1.0.0')).toBe('Older release.')
  })

  it('matches the version exactly, not as a prefix or a pattern', () => {
    expect(extractChangelogSection(CHANGELOG, '1.10.0')).toBe('Newer release.')
    expect(() => extractChangelogSection(CHANGELOG, '1.1')).toThrow('no "## [1.1]" section')
    expect(() => extractChangelogSection(CHANGELOG, '1x1.0')).toThrow('no "## [1x1.0]"')
  })

  it('handles Windows line endings', () => {
    expect(extractChangelogSection(CHANGELOG.replace(/\n/g, '\r\n'), '1.0.0')).toBe(
      'Older release.'
    )
  })

  it('refuses a missing or empty section', () => {
    expect(() => extractChangelogSection(CHANGELOG, '2.0.0')).toThrow('no "## [2.0.0]" section')
    expect(() => extractChangelogSection(CHANGELOG, 'Unreleased')).toThrow('is empty')
  })
})

describe('promoteHeadings', () => {
  it('moves sub-headings up one level', () => {
    expect(promoteHeadings('### Highlights\n#### Detail\ntext ### not a heading')).toBe(
      '## Highlights\n### Detail\ntext ### not a heading'
    )
  })

  it('leaves fenced code blocks alone', () => {
    const markdown = '```sh\n### a shell comment\n```\n### Fixes'
    expect(promoteHeadings(markdown)).toBe('```sh\n### a shell comment\n```\n## Fixes')
  })
})

describe('renderReleaseNotes', () => {
  it('appends the footer with every version placeholder filled in', () => {
    const footer = '## Downloads\n\n`App_{{version}}.exe` and `App_{{version}}.dmg`\n'
    expect(renderReleaseNotes({ changelog: CHANGELOG, footer, version: '1.0.0' })).toBe(
      'Older release.\n\n## Downloads\n\n`App_1.0.0.exe` and `App_1.0.0.dmg`\n'
    )
  })
})
