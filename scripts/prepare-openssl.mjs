#!/usr/bin/env node
/**
 * Stages OpenSSL at install time, so a bare `cargo build` — or rust-analyzer, which
 * never goes through an npm script — finds the archive `.cargo/config.toml` points at.
 *
 * Never fails the install: somebody who only touches the renderer still needs
 * `npm install` to succeed, and a missing OpenSSL surfaces clearly at build time.
 */
import { resolveOpenSslEnv } from './openssl-linkage.mjs'

try {
  resolveOpenSslEnv()
} catch (error) {
  console.warn(`Skipping the OpenSSL setup: ${error.message}`)
}
