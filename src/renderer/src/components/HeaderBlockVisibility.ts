/** Which pieces of `HeaderBlock` are shown — toggled per-window from `HeaderBlockVisibilitySettings`. */
export interface HeaderBlockVisibility {
  clock: boolean
  timecode: boolean
  lineup: boolean
}

export const DEFAULT_HEADER_BLOCK_VISIBILITY: HeaderBlockVisibility = {
  clock: true,
  timecode: true,
  lineup: true
}
