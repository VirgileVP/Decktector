/**
 * Whether the loaded show can drive the timecode output as it stands: the
 * readiness line the editor and Settings both show, the frame-rate check
 * that keeps the output cut until the show and the output agree, and the
 * fallback start that stops fitting after an fps change.
 *
 * Nothing is ever converted between rates: a mismatch is reported, and the
 * timecode service sends nothing until the operator aligns them.
 */

import type { FrameRate } from '../model'
import { countLabel } from './count-label'
import type { ShowData } from './show'
import { frameRateOfFps, parseTimecode, type ShowFps } from './show-timecode'
import type { ShowAnalysis } from './show-validation'

/**
 * The show's fps when it differs from the output's `outputRate` — the rate
 * to offer switching the output to — or null when they agree or there is no
 * show to disagree with.
 */
export function frameRateMismatch(show: ShowData | null, outputRate: FrameRate): ShowFps | null {
  if (show === null) return null
  return frameRateOfFps(show.fps) === outputRate ? null : show.fps
}

/** "Ready", or how many conflicts stand in the way. */
export function readinessLabel(analysis: ShowAnalysis): string {
  return analysis.ready ? 'Ready' : `Not ready — ${countLabel(analysis.conflicts, 'conflict')}`
}

/**
 * Whether the fallback start is a timecode at the show's fps. A label typed
 * at a higher rate keeps its text when the fps drops, and is then pushed as
 * the default fallback until corrected.
 */
export function fallbackStartFits(show: ShowData): boolean {
  return parseTimecode(show.fallbackStartTimecode, show.fps) !== null
}
