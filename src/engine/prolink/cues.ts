import { CueColor, HotcueButton, type CueAndLoop } from 'prolink-connect'

import type { TrackCue } from '../types'

/** Rekordbox-like defaults when a cue has no custom color. */
const DEFAULT_COLORS = {
  memory: '#ff9f2e',
  hotcue: '#12d17e',
  loop: '#ff8a00',
  hotloop: '#ff8a00'
} as const

/** Approximate CSS colors for the palette CDJs assign to cue points. */
const CUE_COLOR_MAP: Partial<Record<CueColor, string>> = {
  [CueColor.Magenta]: '#ff00ff',
  [CueColor.Violet]: '#9d5cff',
  [CueColor.Fuchsia]: '#f45cff',
  [CueColor.LightSlateBlue]: '#8470ff',
  [CueColor.Blue]: '#0a84ff',
  [CueColor.SteelBlue]: '#4682b4',
  [CueColor.Aqua]: '#00d4ff',
  [CueColor.SeaGreen]: '#2e8b57',
  [CueColor.Teal]: '#00a3a3',
  [CueColor.Green]: '#00c853',
  [CueColor.Lime]: '#a3e635',
  [CueColor.Olive]: '#9a9a00',
  [CueColor.Yellow]: '#ffd60a',
  [CueColor.Orange]: '#ff9500',
  [CueColor.Red]: '#ff453a',
  [CueColor.Pink]: '#ff2d92'
}

function buttonLetter(button: HotcueButton): string {
  return HotcueButton[button] ?? '?'
}

/** Maps prolink-connect cue/loop entries to the normalized TrackCue model. */
export function mapCues(cueAndLoops: CueAndLoop[] | null): TrackCue[] {
  if (!cueAndLoops) return []

  return cueAndLoops
    .map((cue): TrackCue => {
      const kind: TrackCue['kind'] =
        cue.type === 'cue_point'
          ? 'memory'
          : cue.type === 'hot_cue'
            ? 'hotcue'
            : cue.type === 'hot_loop'
              ? 'hotloop'
              : 'loop'

      const customColor =
        'color' in cue && cue.color !== undefined ? CUE_COLOR_MAP[cue.color] : undefined

      return {
        kind,
        offsetMs: cue.offset,
        lengthMs: 'length' in cue ? cue.length : null,
        button: 'button' in cue ? buttonLetter(cue.button) : null,
        label: ('label' in cue ? cue.label : undefined) || null,
        color: customColor ?? DEFAULT_COLORS[kind]
      }
    })
    .sort((a, b) => a.offsetMs - b.offsetMs)
}
