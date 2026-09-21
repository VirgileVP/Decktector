#!/usr/bin/env node
/**
 * Runs a Cargo-backed command with the OpenSSL linkage `bundled-sqlcipher` needs, so a
 * fresh Windows checkout builds and runs with no manual setup. A pass-through elsewhere.
 */
import { spawnSync } from 'node:child_process'
import { resolveOpenSslEnv } from './openssl-linkage.mjs'

const argv = process.argv.slice(2)
if (argv.length === 0) {
  console.error('Usage: node scripts/with-openssl.mjs <command> [args...]')
  process.exit(2)
}

let env
try {
  env = { ...process.env, ...resolveOpenSslEnv() }
} catch (error) {
  console.error(error.message)
  process.exit(1)
}

const quoteIfNeeded = (argument) =>
  /[\s"]/.test(argument) ? `"${argument.replace(/"/g, '\\"')}"` : argument

// A single command line, so `shell` can resolve the `.cmd` shims npm puts on PATH for
// local binaries without Node concatenating an argument list behind our back.
const commandLine = argv.map(quoteIfNeeded).join(' ')
const { signal, status } = spawnSync(commandLine, { env, shell: true, stdio: 'inherit' })
if (signal) process.kill(process.pid, signal)
process.exit(status ?? 1)
