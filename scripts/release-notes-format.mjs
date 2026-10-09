/**
 * Builds a GitHub release body from `CHANGELOG.md` — pure text in, text out, so the
 * CLI in `release-notes.mjs` only has to read the files and print.
 *
 * The changelog keeps one `## [<version>] - <date>` section per release, its own
 * sub-headings one level down (`### Highlights`). A release page has no version
 * heading above them, so they move back up a level there, and the shared footer
 * (downloads table, signing notice) goes underneath with `{{version}}` filled in.
 */

const VERSION_PLACEHOLDER = /\{\{version\}\}/g
const SECTION_HEADING = /^## /
const SUB_HEADING = /^(#{3,6}) /
const CODE_FENCE = /^\s*(```|~~~)/

const escapeRegExp = (text) => text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')

/**
 * Returns the body of the changelog section for `version`, without its heading.
 * Throws when the section is missing or empty, so a release can't go out without notes.
 */
export function extractChangelogSection(changelog, version) {
  const heading = new RegExp(`^## \\[${escapeRegExp(version)}\\](\\s|$)`)
  const lines = changelog.split(/\r?\n/)
  const start = lines.findIndex((line) => heading.test(line))
  if (start === -1) {
    throw new Error(`CHANGELOG.md has no "## [${version}]" section`)
  }

  const rest = lines.slice(start + 1)
  const end = rest.findIndex((line) => SECTION_HEADING.test(line))
  const body = (end === -1 ? rest : rest.slice(0, end)).join('\n').trim()
  if (body === '') {
    throw new Error(`The "## [${version}]" section of CHANGELOG.md is empty`)
  }
  return body
}

/** Moves every heading up one level, leaving fenced code blocks untouched. */
export function promoteHeadings(markdown) {
  let inFence = false
  return markdown
    .split('\n')
    .map((line) => {
      if (CODE_FENCE.test(line)) inFence = !inFence
      if (inFence) return line
      return line.replace(SUB_HEADING, (_, hashes) => `${hashes.slice(1)} `)
    })
    .join('\n')
}

/** The full release body: the version's changelog section, then the shared footer. */
export function renderReleaseNotes({ changelog, footer, version }) {
  const section = promoteHeadings(extractChangelogSection(changelog, version))
  const filledFooter = footer.replace(VERSION_PLACEHOLDER, version).trim()
  return `${section}\n\n${filledFooter}\n`
}
