import { isRecord } from './is-record'

/**
 * Pure rules behind the "new version available" notice: reading GitHub's
 * latest-release answer, comparing versions, and remembering which version
 * the user chose to skip.
 */

/** GitHub's latest published release — drafts and the CI's `build-*` prereleases are excluded. */
export const LATEST_RELEASE_API_URL =
  'https://api.github.com/repos/VirgileVP/Decktector/releases/latest'

/** Per-device record of the newest version the user dismissed, kept in `localStorage`. */
export const DISMISSED_UPDATE_STORAGE_KEY = 'decktector.dismissedUpdate'

/** Release tags are `vMAJOR.MINOR.PATCH`, enforced by the CI's release check. */
const RELEASE_TAG_PATTERN = /^v?(\d+)\.(\d+)\.(\d+)$/

type VersionParts = readonly [number, number, number]

function parseVersion(version: string): VersionParts | null {
  const match = RELEASE_TAG_PATTERN.exec(version.trim())
  if (match === null) return null
  return [Number(match[1]), Number(match[2]), Number(match[3])]
}

/** The `MAJOR.MINOR.PATCH` version in a release tag such as `v1.6.0`, or `null` if it has another shape. */
export function versionFromTag(tag: string): string | null {
  const parts = parseVersion(tag)
  return parts === null ? null : parts.join('.')
}

/** Whether `candidate` is strictly newer than `current`. False when either cannot be read. */
export function isNewerVersion(candidate: string, current: string): boolean {
  const candidateParts = parseVersion(candidate)
  const currentParts = parseVersion(current)
  if (candidateParts === null || currentParts === null) return false
  for (let index = 0; index < candidateParts.length; index++) {
    if (candidateParts[index] !== currentParts[index]) {
      return candidateParts[index] > currentParts[index]
    }
  }
  return false
}

/** The version of the release in GitHub's latest-release JSON, or `null` if the answer is unusable. */
export function parseLatestRelease(body: unknown): string | null {
  if (!isRecord(body) || typeof body.tag_name !== 'string') return null
  return versionFromTag(body.tag_name)
}

/**
 * The version to announce: `latest` when it is newer than the running app
 * and not the one the user already dismissed, `null` otherwise.
 */
export function updateToAnnounce(
  latest: string | null,
  current: string | null,
  dismissed: string | null
): string | null {
  if (latest === null || current === null) return null
  if (!isNewerVersion(latest, current)) return null
  return latest === dismissed ? null : latest
}
