#!/usr/bin/env node
/**
 * Prints the GitHub release notes for a version: its `CHANGELOG.md` section followed by
 * `.github/release-footer.md`. Fails when the changelog has no notes for that version,
 * which is how the `Build` workflow refuses to publish a release without them.
 *
 * Usage: node scripts/release-notes.mjs <version>   (e.g. 1.4.0, no leading "v")
 */
import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { renderReleaseNotes } from './release-notes-format.mjs'

const repoRoot = join(dirname(fileURLToPath(import.meta.url)), '..')
const CHANGELOG_PATH = join(repoRoot, 'CHANGELOG.md')
const FOOTER_PATH = join(repoRoot, '.github', 'release-footer.md')

const [version] = process.argv.slice(2)
if (!version) {
  console.error('Usage: node scripts/release-notes.mjs <version>')
  process.exit(2)
}

try {
  const notes = renderReleaseNotes({
    changelog: readFileSync(CHANGELOG_PATH, 'utf8'),
    footer: readFileSync(FOOTER_PATH, 'utf8'),
    version
  })
  process.stdout.write(notes)
} catch (error) {
  console.error(error.message)
  process.exit(1)
}
