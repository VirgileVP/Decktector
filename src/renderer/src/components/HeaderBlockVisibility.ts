import type { LineupView } from '../state/settings'

/** Which pieces of `HeaderBlock` are shown — toggled per-window from `HeaderBlockVisibilityToggle`. */
export interface HeaderBlockVisibility {
  clock: boolean
  timecode: boolean
  lineup: boolean
  /** Each set's note under its name — only a panel window ever shows them. */
  notes: boolean
}

export const DEFAULT_HEADER_BLOCK_VISIBILITY: HeaderBlockVisibility = {
  clock: true,
  timecode: true,
  lineup: true,
  notes: true
}

/**
 * Everything a "Time" panel window chooses for itself: which pieces show,
 * and how much of the running order — independent of the main header's
 * `lineupView` setting.
 */
export interface HeaderBlockDisplay extends HeaderBlockVisibility {
  lineupView: LineupView
}

export const DEFAULT_HEADER_BLOCK_DISPLAY: HeaderBlockDisplay = {
  ...DEFAULT_HEADER_BLOCK_VISIBILITY,
  lineupView: 'full'
}
