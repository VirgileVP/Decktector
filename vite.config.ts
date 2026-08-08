import { resolve } from 'path'
import { defineConfig } from 'vitest/config'
import react from '@vitejs/plugin-react'

/** Fixed so Tauri's devUrl in tauri.conf.json can point at it. */
const DEV_PORT = 1420

export default defineConfig({
  root: 'src/renderer',
  // Tauri serves the built assets from the bundle root, not from a host.
  base: './',
  resolve: {
    alias: {
      '@renderer': resolve(__dirname, 'src/renderer/src')
    }
  },
  plugins: [react()],
  build: {
    outDir: resolve(__dirname, 'dist'),
    emptyOutDir: true,
    // The webview ships with the app, so there is no old browser to support.
    target: 'esnext'
  },
  server: {
    port: DEV_PORT,
    // Fail loudly instead of silently moving to another port Tauri won't load.
    strictPort: true,
    // Vite defaults to binding localhost only. In dev, the remote server
    // redirects unbundled asset requests here (see `AssetSource::DevServer`
    // on the Rust side) — a phone on the LAN needs this reachable too, not
    // just the machine running `npm run dev`.
    host: true
  },
  // Vite hides these from the client by default; Tauri's own vars are safe.
  envPrefix: ['VITE_', 'TAURI_'],
  test: {
    // Every unit under test here is pure logic — no DOM, no jsdom cost.
    environment: 'node',
    include: ['src/renderer/src/**/*.test.ts'],
    // No tests exist yet in a fresh checkout; that is not a failure.
    passWithNoTests: true
  }
})
