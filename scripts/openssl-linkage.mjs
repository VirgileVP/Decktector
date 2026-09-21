/**
 * Resolves the OpenSSL linkage that `bundled-sqlcipher` needs, per platform.
 *
 * The `prolink` crate reads Rekordbox's SQLCipher-encrypted `exportExt.pdb`, so
 * `libsqlite3-sys` is built with the `bundled-sqlcipher` feature and has to reach a
 * libcrypto. How it finds one differs per platform:
 *
 * - Windows: nothing is discovered automatically — the build script panics unless the
 *   OpenSSL directories are handed to it. It then hardcodes the library name
 *   `libcrypto` and links it as a dylib, which would make the packaged app depend on a
 *   `libcrypto-*-x64.dll` at runtime. Staging OpenSSL's *static* archive under that
 *   expected name links the crypto code straight into the executable instead, so the
 *   installer stays self-contained.
 * - Linux: it links the system `libcrypto` — present as soon as the OpenSSL
 *   development package is, which this module only checks for, to turn a bare
 *   `cannot find -lcrypto` into an actionable message.
 * - macOS: SQLCipher falls back to CommonCrypto, so there is nothing to resolve.
 */
import { spawnSync } from 'node:child_process'
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  renameSync,
  rmSync,
  statSync,
  utimesSync
} from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const repoRoot = dirname(dirname(fileURLToPath(import.meta.url)))

/** Where the Win64 development package installs, newest packaging layout first. */
const installRoots = ['C:/Program Files/OpenSSL-Win64', 'C:/Program Files/OpenSSL']

/**
 * Static archive paths, relative to an installation root, one per packaging generation.
 * `MD` matches the dynamic CRT that the stable Rust MSVC toolchain links against; the
 * flat `lib/VC/static` layout predates ARM64 support, so it is x64 only.
 */
const staticArchives = (architecture) => [
  `lib/VC/${architecture}/MD/libcrypto_static.lib`,
  ...(architecture === 'x64' ? ['lib/VC/static/libcrypto64MD.lib'] : [])
]

const stagingDir = join(repoRoot, 'target', 'openssl-static', 'lib')

/** The archive name `libsqlite3-sys` hands to the linker. */
const stagedArchive = join(stagingDir, 'libcrypto.lib')

const wingetPackageId = 'ShiningLight.OpenSSL.Dev'
const manualInstallHint = `winget install --id ${wingetPackageId} -e`

/** Where the App Installer package puts `winget`, for shells that lack it on PATH. */
const wingetFallbackPath = join(process.env.LOCALAPPDATA ?? '', 'Microsoft/WindowsApps/winget.exe')

/** Directories a Linux toolchain looks in for the `libcrypto.so` link-time symlink. */
const linuxLibraryDirs = [
  '/usr/lib',
  '/usr/lib64',
  '/usr/local/lib',
  `/usr/lib/${process.arch === 'arm64' ? 'aarch64' : 'x86_64'}-linux-gnu`
]

const linuxPackageHint =
  'libssl-dev (Debian/Ubuntu), openssl-devel (Fedora/RHEL) or openssl (Arch/Alpine)'

/** The MSVC target whose build `libsqlite3-sys` reads target-prefixed variables for. */
function windowsTarget() {
  return process.arch === 'arm64' ? 'aarch64-pc-windows-msvc' : 'x86_64-pc-windows-msvc'
}

function findInstallation() {
  const architecture = process.arch === 'arm64' ? 'arm64' : 'x64'
  const roots = [process.env.OPENSSL_DIR, ...installRoots].filter(Boolean)
  for (const root of roots) {
    const includeDir = join(root, 'include')
    if (!existsSync(includeDir)) continue
    const archive = staticArchives(architecture)
      .map((path) => join(root, path))
      .find(existsSync)
    if (archive) return { archive, includeDir }
  }
  return null
}

function installDevelopmentPackage() {
  console.log(`OpenSSL development package not found — installing ${wingetPackageId}…`)
  const winget = existsSync(wingetFallbackPath) ? wingetFallbackPath : 'winget.exe'
  const { error, status } = spawnSync(
    winget,
    [
      'install',
      '--id',
      wingetPackageId,
      '-e',
      '--accept-package-agreements',
      '--accept-source-agreements'
    ],
    { stdio: 'inherit' }
  )
  if (error || status !== 0) {
    throw new Error(
      `Could not install OpenSSL automatically. Install it with: ${manualInstallHint}`
    )
  }
}

/**
 * Copies the archive under the name the linker expects, keeping the source timestamp so
 * the copy can be skipped next time. Written to a private path and renamed into place,
 * because two npm scripts may stage the same 60 MB archive at once and a build that
 * picked up a half-written one would fail deep in the linker.
 */
function stageArchive(archive) {
  const source = statSync(archive)
  const isStale = !existsSync(stagedArchive) || statSync(stagedArchive).mtimeMs !== source.mtimeMs
  if (!isStale) return stagingDir

  mkdirSync(stagingDir, { recursive: true })
  const pendingArchive = `${stagedArchive}.${process.pid}.tmp`
  try {
    copyFileSync(archive, pendingArchive)
    utimesSync(pendingArchive, source.atime, source.mtime)
    renameSync(pendingArchive, stagedArchive)
  } catch (error) {
    rmSync(pendingArchive, { force: true })
    throw error
  }
  return stagingDir
}

function resolveWindowsEnv() {
  let installation = findInstallation()
  if (!installation) {
    installDevelopmentPackage()
    installation = findInstallation()
    if (!installation) {
      throw new Error(
        `OpenSSL is installed but no ${process.arch} static libcrypto was found under ` +
          `${installRoots.join(' or ')}. Install the development package with: ${manualInstallHint}`
      )
    }
  }

  const libDir = stageArchive(installation.archive)
  const includeDir = installation.includeDir

  // `libsqlite3-sys` reads `<TARGET>_<NAME>` before `<NAME>`, and `.cargo/config.toml`
  // sets the target-prefixed pair so a bare `cargo build` and rust-analyzer work too.
  // Setting both forms here keeps an OpenSSL installed elsewhere winning over that
  // default, whichever form the build script ends up reading.
  const targetPrefix = windowsTarget().toUpperCase().replaceAll('-', '_')
  return {
    OPENSSL_LIB_DIR: libDir,
    OPENSSL_INCLUDE_DIR: includeDir,
    [`${targetPrefix}_OPENSSL_LIB_DIR`]: libDir,
    [`${targetPrefix}_OPENSSL_INCLUDE_DIR`]: includeDir
  }
}

/**
 * Warns when no link-time libcrypto is in sight. Only a warning: the search covers the
 * usual multiarch layouts, not every distribution's, and a false alarm must not stop a
 * build that would have linked fine.
 */
function warnIfLinuxDevelopmentPackageMissing() {
  const pkgConfig = spawnSync('pkg-config', ['--exists', 'libcrypto'])
  const foundByPkgConfig = !pkgConfig.error && pkgConfig.status === 0
  if (foundByPkgConfig) return
  if (linuxLibraryDirs.some((dir) => existsSync(join(dir, 'libcrypto.so')))) return

  console.warn(
    `No link-time libcrypto found — SQLCipher will fail to link. Install ${linuxPackageHint}.`
  )
}

/**
 * Environment variables `libsqlite3-sys` needs to link libcrypto, installing and staging
 * OpenSSL as needed. Empty wherever the build script resolves libcrypto by itself.
 */
export function resolveOpenSslEnv() {
  if (process.platform === 'win32') return resolveWindowsEnv()
  if (process.platform === 'linux') warnIfLinuxDevelopmentPackageMissing()
  return {}
}
