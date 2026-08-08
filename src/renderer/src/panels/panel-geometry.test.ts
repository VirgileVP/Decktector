import { describe, expect, it } from 'vitest'
import { clampToMonitors, type MonitorBounds, type WindowGeometry } from './panel-geometry'

const MIN_SIZE = { width: 160, height: 80 }

const PRIMARY: MonitorBounds = { x: 0, y: 0, width: 1920, height: 1080 }
const SECONDARY_LEFT: MonitorBounds = { x: -1920, y: 0, width: 1920, height: 1080 }
const SMALL_LAPTOP: MonitorBounds = { x: 0, y: 0, width: 1280, height: 720 }

describe('clampToMonitors', () => {
  it('leaves a geometry fully inside a single monitor unchanged', () => {
    const geometry: WindowGeometry = { x: 100, y: 100, width: 400, height: 300 }
    expect(clampToMonitors(geometry, [PRIMARY], MIN_SIZE)).toEqual(geometry)
  })

  it('moves a geometry fully outside every monitor onto monitors[0]', () => {
    const geometry: WindowGeometry = { x: 5000, y: 5000, width: 400, height: 300 }
    const result = clampToMonitors(geometry, [PRIMARY, SMALL_LAPTOP], MIN_SIZE)
    expect(result.x + result.width).toBeLessThanOrEqual(PRIMARY.x + PRIMARY.width)
    expect(result.x).toBeGreaterThanOrEqual(PRIMARY.x)
    expect(result.y + result.height).toBeLessThanOrEqual(PRIMARY.y + PRIMARY.height)
    expect(result.y).toBeGreaterThanOrEqual(PRIMARY.y)
  })

  it('lands on the monitor with the larger overlap when straddling two', () => {
    // Mostly on SECONDARY_LEFT (x: -1920..0), a sliver on PRIMARY (x: 0..1920).
    const geometry: WindowGeometry = { x: -1800, y: 100, width: 1850, height: 300 }
    const result = clampToMonitors(geometry, [PRIMARY, SECONDARY_LEFT], MIN_SIZE)
    expect(result.x).toBeGreaterThanOrEqual(SECONDARY_LEFT.x)
    expect(result.x + result.width).toBeLessThanOrEqual(SECONDARY_LEFT.x + SECONDARY_LEFT.width)
  })

  it('shrinks a geometry larger than the monitor down to it', () => {
    const geometry: WindowGeometry = { x: 0, y: 0, width: 3000, height: 2000 }
    const result = clampToMonitors(geometry, [SMALL_LAPTOP], MIN_SIZE)
    expect(result.width).toBe(SMALL_LAPTOP.width)
    expect(result.height).toBe(SMALL_LAPTOP.height)
  })

  it('grows a geometry smaller than minSize up to it', () => {
    const geometry: WindowGeometry = { x: 0, y: 0, width: 50, height: 30 }
    const result = clampToMonitors(geometry, [PRIMARY], MIN_SIZE)
    expect(result.width).toBe(MIN_SIZE.width)
    expect(result.height).toBe(MIN_SIZE.height)
  })

  it('preserves legal negative coordinates on a monitor left of the primary', () => {
    const geometry: WindowGeometry = { x: -1900, y: 50, width: 800, height: 400 }
    expect(clampToMonitors(geometry, [SECONDARY_LEFT, PRIMARY], MIN_SIZE)).toEqual(geometry)
  })

  it('returns the geometry unchanged for an empty monitor list', () => {
    const geometry: WindowGeometry = { x: 42, y: 42, width: 400, height: 300 }
    expect(clampToMonitors(geometry, [], MIN_SIZE)).toEqual(geometry)
  })
})
