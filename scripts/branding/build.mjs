#!/usr/bin/env node
/**
 * Regenerates every brand asset from the parametric mark: the logo tiles and lockups in
 * branding/, and the app icon master src-tauri/app-icon.png.
 *
 * Usage: npm run branding
 * Then refresh the platform icons: npm run tauri -- icon src-tauri/app-icon.png
 * (delete the android/ and ios/ folders it adds; the app does not ship them).
 */
import { mkdirSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { CENTER, SIZE, markBody } from './mark.mjs'
import { APP_ICON_TILE, colorsFor } from './palette.mjs'
import { inkBounds, renderPng, svgDocument } from './svg.mjs'
import { lockup } from './wordmark.mjs'

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..', '..')
const BRANDING_DIR = join(ROOT, 'branding')
const APP_ICON_PNG = join(ROOT, 'src-tauri', 'app-icon.png')

const LOCKUP_PNG_WIDTH = 2400
// Clear space around the lockup, as a fraction of its visible height
const LOCKUP_CLEAR_SPACE = 0.35

const APP_ICON = {
  canvas: 1024,
  // Apple icon grid: an 824 px tile on a 1024 px canvas, corner radius 22.37% of the tile
  tile: 824,
  cornerRatio: 0.2237
}

function writeAsset(path, svg, pngWidth) {
  writeFileSync(`${path}.svg`, svg)
  writeFileSync(`${path}.png`, renderPng(svg, pngWidth))
}

/** Square mark on a full-bleed background; the dark one is the Instagram avatar. */
function buildMarkTiles() {
  for (const mode of ['dark', 'light']) {
    const colors = colorsFor(mode)
    const body = `<rect width="${SIZE}" height="${SIZE}" fill="${colors.bg}"/>${markBody(colors)}`
    writeAsset(join(BRANDING_DIR, `logo-mark-${mode}`), svgDocument(SIZE, SIZE, body), SIZE)
  }
}

/** Horizontal lockups, cropped to the artwork plus uniform clear space. */
function buildLockups() {
  const variants = [
    { name: 'dark', colors: colorsFor('dark'), withBackground: true },
    { name: 'light', colors: colorsFor('light'), withBackground: true },
    // Transparent: light ink for photos and dark media, dark ink for light media
    { name: 'white', colors: colorsFor('dark'), withBackground: false },
    { name: 'black', colors: colorsFor('light'), withBackground: false }
  ]
  const margin = 100
  const probe = lockup(colorsFor('dark'), 1)
  const bounds = inkBounds(
    svgDocument(
      Math.ceil(probe.width) + 2 * margin,
      Math.ceil(probe.height) + 2 * margin,
      probe.draw(margin, margin)
    )
  )
  const pad = bounds.height * LOCKUP_CLEAR_SPACE
  const view = {
    x: bounds.x - pad,
    y: bounds.y - pad,
    width: bounds.width + 2 * pad,
    height: bounds.height + 2 * pad
  }
  for (const variant of variants) {
    const background = variant.withBackground
      ? `<rect x="${view.x}" y="${view.y}" width="${view.width}" height="${view.height}" fill="${variant.colors.bg}"/>`
      : ''
    const body = background + lockup(variant.colors, 1).draw(margin, margin)
    const viewBox = `${view.x} ${view.y} ${view.width} ${view.height}`
    const svg = svgDocument(view.width, view.height, body, viewBox)
    writeAsset(join(BRANDING_DIR, `logo-lockup-${variant.name}`), svg, LOCKUP_PNG_WIDTH)
  }
}

/** Rounded tile on a transparent canvas, the mark scaled to the tile. */
function buildAppIcon() {
  const { canvas, tile, cornerRatio } = APP_ICON
  const origin = (canvas - tile) / 2
  const scale = tile / SIZE
  const body = `<rect x="${origin}" y="${origin}" width="${tile}" height="${tile}" rx="${tile * cornerRatio}" fill="${APP_ICON_TILE}"/><g transform="translate(${canvas / 2} ${canvas / 2}) scale(${scale}) translate(${-CENTER} ${-CENTER})">${markBody(colorsFor('dark'))}</g>`
  const svg = svgDocument(canvas, canvas, body)
  writeFileSync(join(BRANDING_DIR, 'app-icon.svg'), svg)
  writeFileSync(APP_ICON_PNG, renderPng(svg, canvas))
}

mkdirSync(BRANDING_DIR, { recursive: true })
buildMarkTiles()
buildLockups()
buildAppIcon()
console.log('Brand assets written to branding/ and src-tauri/app-icon.png')
