import type { TempoRange } from '@renderer/model/TempoRange'

/** Number of simulated decks; the Rust rig is always four. */
export const DECK_COUNT = 4

/** Hot cue pad letters, in the order the pads are laid out. */
export const HOT_CUE_BUTTONS = ['A', 'B', 'C', 'D', 'E', 'F', 'G', 'H'] as const

/** Loop lengths the 4 BEAT / 8 BEAT buttons set. */
export const LOOP_LENGTHS_BEATS = [4, 8] as const

/** Tempo range options as the CDJ's TEMPO RANGE button offers them. */
export const TEMPO_RANGE_OPTIONS: readonly { range: TempoRange; label: string }[] = [
  { range: 'six', label: '±6' },
  { range: 'ten', label: '±10' },
  { range: 'sixteen', label: '±16' },
  { range: 'wide', label: 'WIDE' }
]

/** Beat boxes shown on the screen (one bar). */
export const BEATS_PER_BAR = 4
