import { clamp } from './jog-math'

/** Travel of a vertical fader, and which end of it sits at the bottom. */
export interface FaderTravel {
  min: number
  max: number
  /** True when `max` is at the bottom (a CDJ pitch fader: pull down to go faster). */
  maxAtBottom: boolean
}

/** Pitch fader of a CDJ: -1 at the top (slower), +1 at the bottom (faster). */
export const PITCH_FADER_TRAVEL: FaderTravel = { min: -1, max: 1, maxAtBottom: true }

/** Channel fader of the mixer: silent at the bottom, full at the top. */
export const CHANNEL_FADER_TRAVEL: FaderTravel = { min: 0, max: 1, maxAtBottom: false }

/** Distance from the top of the track, 0..1, at which `value` is drawn. */
export function fractionFromTop(value: number, travel: FaderTravel): number {
  const fraction = clamp((value - travel.min) / (travel.max - travel.min), 0, 1)
  return travel.maxAtBottom ? fraction : 1 - fraction
}

/** Value for a pointer `fromTop` (0..1) of the way down the track. */
export function valueFromFraction(fromTop: number, travel: FaderTravel): number {
  const fraction = clamp(fromTop, 0, 1)
  const along = travel.maxAtBottom ? fraction : 1 - fraction
  return travel.min + along * (travel.max - travel.min)
}
