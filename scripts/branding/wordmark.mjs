/** "Decktector" wordmark outlined to SVG paths, and the horizontal lockup with the mark. */
import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import opentype from 'opentype.js'
import { CENTER, LENS_INNER_RADIUS, MARK_HALF_WIDTH, SIZE, markBody } from './mark.mjs'

const WORDMARK = 'Decktector'
// Geist SemiBold, static cut: opentype.js reads WOFF but not the variable WOFF2 the app bundles
const FONT_FILE = '@fontsource/geist/files/geist-latin-600-normal.woff'
// Display tracking, in em
const TRACKING = -0.03
// Wordmark cap height as a fraction of the lens inner diameter
const CAP_TO_LENS = 0.62
// Gap between mark and wordmark as a fraction of the mark width
const GAP_TO_MARK = 0.18

function loadFont() {
  const buffer = readFileSync(createRequire(import.meta.url).resolve(FONT_FILE))
  return opentype.parse(
    buffer.buffer.slice(buffer.byteOffset, buffer.byteOffset + buffer.byteLength)
  )
}

const font = loadFont()

/** Outlines the wordmark glyph by glyph with kerning and tracking, baseline at y = 0. */
function outlineWordmark(fontSize) {
  const scale = fontSize / font.unitsPerEm
  // Per-character lookup: a plain Latin word needs no OpenType shaping
  const glyphs = [...WORDMARK].map((char) => font.charToGlyph(char))
  let x = 0
  let pathData = ''
  glyphs.forEach((glyph, i) => {
    pathData += glyph.getPath(x, 0, fontSize).toPathData(2)
    const kerning = i < glyphs.length - 1 ? font.getKerningValue(glyph, glyphs[i + 1]) : 0
    x += (glyph.advanceWidth + kerning) * scale + TRACKING * fontSize
  })
  return {
    pathData,
    width: x - TRACKING * fontSize,
    capHeight: font.tables.os2.sCapHeight * scale
  }
}

/**
 * Horizontal lockup: the mark, then the wordmark with its cap band centered on the lens axis.
 * Returns its layout box and a function drawing it with the top-left corner at (x, y).
 *
 * @param {{ ink: string, accent: string }} colors
 * @param {number} markScale mark size relative to its SIZE×SIZE frame
 */
export function lockup(colors, markScale) {
  const targetCapHeight = LENS_INNER_RADIUS * 2 * markScale * CAP_TO_LENS
  const fontSize = (100 * targetCapHeight) / outlineWordmark(100).capHeight
  const text = outlineWordmark(fontSize)
  const markWidth = MARK_HALF_WIDTH * 2 * markScale
  const gap = GAP_TO_MARK * markWidth
  const height = SIZE * markScale
  return {
    width: markWidth + gap + text.width,
    height,
    draw: (x, y) => {
      const axisY = y + height / 2
      const mark = `<g transform="translate(${x + markWidth / 2} ${axisY}) scale(${markScale}) translate(${-CENTER} ${-CENTER})">${markBody(colors)}</g>`
      const wordmark = `<path transform="translate(${x + markWidth + gap} ${axisY + text.capHeight / 2})" d="${text.pathData}" fill="${colors.ink}"/>`
      return mark + wordmark
    }
  }
}
