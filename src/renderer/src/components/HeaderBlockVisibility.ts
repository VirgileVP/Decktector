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
