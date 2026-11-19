import { useEffect, useId, useRef, useState } from 'react'

import { frameRateMismatch } from '../lib/show-readiness'
import { fpsOfFrameRate, frameRateOfFps, type ShowFps } from '../lib/show-timecode'
import { useSettings } from '../state/settings'
import { useShow } from '../state/show'

/** Names one show/output disagreement, so leaving it cut is remembered for that pair only. */
function mismatchKey(showFps: ShowFps, outputFps: ShowFps): string {
  return `${showFps}/${outputFps}`
}

/**
 * Raised in the main window whenever the loaded show and the timecode output
 * run at different rates. The service already sends nothing meanwhile — no
 * rate is converted — so the operator either switches the output to the
 * show's rate or leaves it cut. Blocking: a click outside or Escape does not
 * dismiss it. Leaving it cut holds for that pair of rates; another mismatch
 * raises it again, and Settings > Show keeps a note until they match.
 */
export function FrameRateMismatchDialog(): React.JSX.Element | null {
  const show = useShow((state) => state.show)
  const outputRate = useSettings((state) => state.timecodeFrameRate)
  const showFps = frameRateMismatch(show, outputRate)
  const outputFps = fpsOfFrameRate(outputRate)
  const key = showFps === null ? null : mismatchKey(showFps, outputFps)
  const [leftCut, setLeftCut] = useState<string | null>(null)

  // Once the rates agree, the next disagreement asks again, even the same one.
  if (key === null && leftCut !== null) setLeftCut(null)

  if (showFps === null || key === leftCut) return null
  return (
    <MismatchPrompt
      showFps={showFps}
      outputFps={outputFps}
      onSwitch={() => useSettings.getState().update('timecodeFrameRate', frameRateOfFps(showFps))}
      onLeaveCut={() => setLeftCut(key)}
    />
  )
}

interface MismatchPromptProps {
  showFps: ShowFps
  outputFps: ShowFps
  onSwitch: () => void
  onLeaveCut: () => void
}

/** The modal itself. Leaving the output cut is the focused default: it changes nothing. */
function MismatchPrompt({
  showFps,
  outputFps,
  onSwitch,
  onLeaveCut
}: MismatchPromptProps): React.JSX.Element {
  const leaveRef = useRef<HTMLButtonElement>(null)
  const titleId = useId()
  const bodyId = useId()

  useEffect(() => {
    leaveRef.current?.focus()
  }, [])

  return (
    <div className="confirm-backdrop">
      <div
        className="confirm-panel"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={bodyId}
        onKeyDown={(event) => {
          // Blocking: Escape must not close this, nor the Settings dialog under it.
          if (event.key === 'Escape') event.stopPropagation()
        }}
      >
        <h3 id={titleId} className="confirm-title">
          The show and the timecode output disagree
        </h3>
        <div id={bodyId} className="confirm-body">
          <p>
            The show runs at {showFps} fps and the output at {outputFps} fps. Nothing is converted:
            the output sends no timecode until both run at the same rate.
          </p>
        </div>
        <div className="confirm-actions">
          <button ref={leaveRef} type="button" className="setting-btn" onClick={onLeaveCut}>
            Leave the output cut
          </button>
          <button type="button" className="setting-btn confirm-accept" onClick={onSwitch}>
            Switch output to {showFps} fps
          </button>
        </div>
      </div>
    </div>
  )
}
