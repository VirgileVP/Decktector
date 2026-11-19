/**
 * What a deck's bar says about its timecode against the show: the row it
 * runs on, or why it falls back, is cut, or waits. A deck on air whose timecode is frozen, at the fallback or
 * silent must be readable in its own bar, not only in logs.
 */

import type { DeckMatchStatus, UnidentifiedReason } from '../model'
import { formatTimecode, type ShowFps } from './show-timecode'

/** How much the operator should worry: on its row, waiting, off its row, sending nothing. */
export type DeckTimecodeTone = 'ok' | 'pending' | 'warning' | 'alert'

export interface DeckTimecodeView {
  tone: DeckTimecodeTone
  text: string
  /** Whether the deck runs on a show row's timeline; its rail badge shows a link or a broken one. */
  linked: boolean
}

/** Rates the line needs to read frames and to name a mismatch. */
export interface DeckTimecodeRates {
  /** The show's fps; start frames are counted at this rate. */
  showFps: ShowFps
  /** The timecode output's fps. */
  outputFps: ShowFps
}

/** Why a deck is not on a row's timeline, in a few words. */
function reasonText(reason: UnidentifiedReason): string {
  switch (reason.type) {
    case 'noMatch':
      return 'unknown track'
    case 'noDuration':
      return 'no duration'
    case 'ambiguous':
      return `ambiguous (${reason.rowIds.length} rows)`
    case 'blankStart':
      return 'not yet timed (blank start)'
    case 'pendingTimeout':
      return 'identification timed out'
  }
}

/** A start in frames as a label, or a placeholder when the service sent none. */
function startLabel(startFrames: number | null, fps: ShowFps): string {
  return startFrames === null ? '--:--:--:--' : formatTimecode(startFrames, fps)
}

/**
 * The line for `status`, or null when there is nothing to say: no track
 * loaded, or no show — every deck then runs from midnight, as before shows.
 */
export function deckTimecodeView(
  status: DeckMatchStatus,
  rates: DeckTimecodeRates
): DeckTimecodeView | null {
  const { resolution } = status
  const view = (tone: DeckTimecodeTone, text: string): DeckTimecodeView => ({
    tone,
    text,
    linked: resolution.type === 'matched'
  })
  switch (resolution.type) {
    case 'none':
    case 'inactive':
      return null
    case 'pending':
      return view('pending', 'identifying…')
    case 'rateMismatch':
      return view('alert', `TC cut · show ${rates.showFps} fps, output ${rates.outputFps} fps`)
    case 'matched':
      return view(
        'ok',
        `▸ ${status.rowTitle ?? 'Untitled'} · ${startLabel(status.startFrames, rates.showFps)}`
      )
    case 'fallback':
      return view(
        'warning',
        `fallback ${startLabel(status.startFrames, rates.showFps)} · ${reasonText(resolution.reason)}`
      )
    case 'cut':
      return view('alert', `TC cut · ${reasonText(resolution.reason)}`)
  }
}
