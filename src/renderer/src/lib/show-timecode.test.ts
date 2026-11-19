import { describe, expect, it } from 'vitest'

import {
  FALLBACK_START_TC_DEFAULT,
  SHOW_FPS_OPTIONS,
  TIMECODE_LABEL_LENGTH,
  formatTimecode,
  fpsOfFrameRate,
  frameRateOfFps,
  framesPerDay,
  keepTimecodeCharacters,
  parseTimecode,
  readTimecodeLabel,
  trimFramesToMs,
  trimMsToFrames
} from './show-timecode'

const SECONDS_PER_HOUR = 3600

describe('parseTimecode', () => {
  it('rejects a frame number at or above fps', () => {
    for (const fps of SHOW_FPS_OPTIONS) {
      const last = String(fps - 1).padStart(2, '0')
      expect(parseTimecode(`00:00:00:${last}`, fps)).toBe(fps - 1)
      expect(parseTimecode(`00:00:00:${fps}`, fps)).toBeNull()
    }
  })

  it('rejects hour 24 and minute 60', () => {
    expect(parseTimecode('24:00:00:00', 25)).toBeNull()
    expect(parseTimecode('00:60:00:00', 25)).toBeNull()
    expect(parseTimecode('00:00:60:00', 25)).toBeNull()
  })

  it('accepts 00:00:00:00 and 23:59:59:29', () => {
    expect(parseTimecode('00:00:00:00', 30)).toBe(0)
    expect(parseTimecode('23:59:59:29', 30)).toBe(framesPerDay(30) - 1)
  })

  it('counts frames from midnight at the given rate', () => {
    expect(parseTimecode('01:00:00:00', 25)).toBe(SECONDS_PER_HOUR * 25)
    expect(parseTimecode('00:01:01:05', 24)).toBe(61 * 24 + 5)
  })

  it('rejects anything but two digits per field', () => {
    for (const text of ['1:2:3', '1:02:03:04', '01:02:03', '01:02:03:04:05', ' 01:02:03:04', '']) {
      expect(parseTimecode(text, 25)).toBeNull()
    }
  })
})

describe('keepTimecodeCharacters', () => {
  it('drops letters and other characters, keeping digits and colons', () => {
    expect(keepTimecodeCharacters('0a1:b3c0: 00;00', TIMECODE_LABEL_LENGTH)).toBe('01:30:0000')
  })

  it('caps the text at the given length', () => {
    expect(keepTimecodeCharacters('01:30:00:00:99', TIMECODE_LABEL_LENGTH)).toBe('01:30:00:00')
    expect(keepTimecodeCharacters('01:30:00:00', 8)).toBe('01:30:00')
  })
})

describe('readTimecodeLabel', () => {
  it('keeps a frame field that no rate accepts, for the fps check to flag', () => {
    expect(readTimecodeLabel('00:10:00:28')).toEqual({
      hours: 0,
      minutes: 10,
      seconds: 0,
      frames: 28
    })
    expect(parseTimecode('00:10:00:28', 25)).toBeNull()
  })
})

describe('formatTimecode', () => {
  it('round trips every frame of an hour at every rate', () => {
    // One assertion per rate: an expect() per frame (~284k) outruns the CI test timeout.
    for (const fps of SHOW_FPS_OPTIONS) {
      const mismatches: number[] = []
      for (let frames = 0; frames < SECONDS_PER_HOUR * fps; frames++) {
        if (parseTimecode(formatTimecode(frames, fps), fps) !== frames) mismatches.push(frames)
      }
      expect(mismatches, `frames that do not round trip at ${fps} fps`).toEqual([])
    }
  })

  it('wraps at 24 hours either way', () => {
    expect(formatTimecode(framesPerDay(25) + 3, 25)).toBe('00:00:00:03')
    expect(formatTimecode(-1, 25)).toBe('23:59:59:24')
  })
})

describe('trim', () => {
  it('is stored in ms and stepped in frames', () => {
    expect(trimFramesToMs(2, 25)).toBe(80)
    expect(trimMsToFrames(80, 25)).toBe(2)
    expect(trimFramesToMs(-2, 25)).toBe(-80)

    // A frame at 30 fps is 33.3 ms: the stored value comes back as a whole frame.
    for (let frames = -10; frames <= 10; frames++) {
      expect(trimMsToFrames(trimFramesToMs(frames, 30), 30)).toBe(frames)
    }
  })
})

describe('FALLBACK_START_TC_DEFAULT', () => {
  it('is a valid label at every rate', () => {
    for (const fps of SHOW_FPS_OPTIONS) {
      expect(parseTimecode(FALLBACK_START_TC_DEFAULT, fps)).not.toBeNull()
    }
  })
})

describe('frameRateOfFps and fpsOfFrameRate', () => {
  it('fps and frame rate names round trip for all three rates', () => {
    for (const fps of SHOW_FPS_OPTIONS) {
      expect(fpsOfFrameRate(frameRateOfFps(fps))).toBe(fps)
    }
  })
})
