#!/usr/bin/env node
/**
 * Fails when an AppImage bundles any binary that needs a newer glibc than the oldest
 * distribution it should run on. The AppImage carries its own libraries but not glibc,
 * so building on a recent host silently drops support for older systems (issue #3).
 *
 * Linux only: needs `objdump` (binutils) and an AppImage able to `--appimage-extract`.
 *
 * Usage: node scripts/appimage-glibc-check.mjs <file.AppImage> <max glibc, e.g. 2.35>
 */
import { execFileSync } from 'node:child_process'
import { chmodSync, mkdtempSync, openSync, readSync, closeSync, readdirSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, relative, resolve } from 'node:path'
import { findGlibcViolations, parseVersion, requiredGlibcVersions } from './glibc-versions.mjs'

const ELF_MAGIC = Buffer.from([0x7f, 0x45, 0x4c, 0x46])
const EXTRACTED_DIR_NAME = 'squashfs-root'

function isElf(path) {
  const header = Buffer.alloc(ELF_MAGIC.length)
  const descriptor = openSync(path, 'r')
  try {
    const bytesRead = readSync(descriptor, header, 0, header.length, 0)
    return bytesRead === header.length && header.equals(ELF_MAGIC)
  } finally {
    closeSync(descriptor)
  }
}

function listRegularFiles(directory) {
  return readdirSync(directory, { withFileTypes: true, recursive: true })
    .filter((entry) => entry.isFile())
    .map((entry) => join(entry.parentPath, entry.name))
}

function extractAppImage(appImagePath, workDirectory) {
  chmodSync(appImagePath, 0o755)
  execFileSync(appImagePath, ['--appimage-extract'], { cwd: workDirectory, stdio: 'ignore' })
  return join(workDirectory, EXTRACTED_DIR_NAME)
}

function readBundledBinaries(rootDirectory) {
  return listRegularFiles(rootDirectory)
    .filter(isElf)
    .map((path) => ({
      file: relative(rootDirectory, path),
      versions: requiredGlibcVersions(execFileSync('objdump', ['-p', path], { encoding: 'utf8' }))
    }))
}

const [appImageArgument, maxVersion] = process.argv.slice(2)
if (!appImageArgument || !maxVersion) {
  console.error('Usage: node scripts/appimage-glibc-check.mjs <file.AppImage> <max glibc>')
  process.exit(2)
}

const workDirectory = mkdtempSync(join(tmpdir(), 'appimage-glibc-'))
try {
  parseVersion(maxVersion)
  const binaries = readBundledBinaries(extractAppImage(resolve(appImageArgument), workDirectory))
  if (binaries.length === 0) {
    throw new Error(`No ELF binaries found in ${appImageArgument}`)
  }

  const violations = findGlibcViolations(binaries, maxVersion)
  if (violations.length > 0) {
    for (const { file, newest } of violations) {
      console.error(`${file} needs GLIBC_${newest}`)
    }
    console.error(
      `${violations.length} bundled binaries need a glibc newer than ${maxVersion}: ` +
        'the AppImage will not start on older distributions.'
    )
    process.exit(1)
  }
  console.log(`${binaries.length} bundled binaries all run on glibc ${maxVersion}.`)
} catch (error) {
  console.error(error.message)
  process.exit(1)
} finally {
  rmSync(workDirectory, { recursive: true, force: true })
}
