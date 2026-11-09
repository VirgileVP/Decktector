/**
 * Pure helpers for reading which glibc versions an ELF binary requires, from the
 * "Version References" that `objdump -p` prints. Kept apart from the CLI in
 * `appimage-glibc-check.mjs` so it can be tested without a Linux toolchain.
 */

const GLIBC_VERSION_PATTERN = /\bGLIBC_(\d+(?:\.\d+)+)\b/g

/**
 * Parses a dotted version ("2.35", "2.2.5") into its numeric parts.
 * @param {string} version
 * @returns {number[]}
 */
export function parseVersion(version) {
  if (!/^\d+(?:\.\d+)*$/.test(version)) {
    throw new Error(`Not a dotted version: "${version}"`)
  }
  return version.split('.').map(Number)
}

/**
 * Orders two dotted versions numerically, so "2.10" sorts after "2.9".
 * @returns {number} negative when `a` is older, positive when newer, 0 when equal
 */
export function compareVersions(a, b) {
  const left = parseVersion(a)
  const right = parseVersion(b)
  for (let index = 0; index < Math.max(left.length, right.length); index++) {
    const difference = (left[index] ?? 0) - (right[index] ?? 0)
    if (difference !== 0) return difference
  }
  return 0
}

/**
 * Lists the distinct glibc versions named in `objdump -p` output, oldest first.
 * @param {string} objdumpOutput
 * @returns {string[]}
 */
export function requiredGlibcVersions(objdumpOutput) {
  const versions = new Set()
  for (const match of objdumpOutput.matchAll(GLIBC_VERSION_PATTERN)) {
    versions.add(match[1])
  }
  return [...versions].sort(compareVersions)
}

/**
 * Keeps the binaries whose newest required glibc is above `maxVersion`.
 * @param {{ file: string, versions: string[] }[]} binaries
 * @param {string} maxVersion
 * @returns {{ file: string, newest: string }[]}
 */
export function findGlibcViolations(binaries, maxVersion) {
  return binaries
    .filter(({ versions }) => versions.length > 0)
    .map(({ file, versions }) => ({ file, newest: versions[versions.length - 1] }))
    .filter(({ newest }) => compareVersions(newest, maxVersion) > 0)
}
