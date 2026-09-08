import { useRef } from 'react'

import {
  DEFAULT_HEADER_BLOCK_VISIBILITY,
  type HeaderBlockVisibility
} from './HeaderBlockVisibility'
import { HeaderClock } from './HeaderClock'
import { LineupNow } from './LineupNow'
import { PopOutButton } from './PopOutButton'
import { TimecodeClock } from './TimecodeClock'
import { TimecodeSourceButtons } from './TimecodeSourceButtons'
import { useFitToBox } from './useFitToBox'

interface HeaderBlockProps {
  /** Defaults to showing everything — the main header always does. A panel
   * window controls this through its own chrome (`HeaderBlockVisibilityToggle`). */
  visibility?: HeaderBlockVisibility
  /**
   * Scales the block to fill whatever space it is given, and lets its shape
   * choose the layout. Panel windows turn this on — the block is the whole
   * window there; the app header leaves it off and keeps its fixed size.
   */
  fitToWindow?: boolean
}

/**
 * The clocks-plus-lineup block shown in the header — also popped out as its
 * own panel window (titled "Time" there). The clock and the timecode have
 * no pop-out of their own: only this combined window can be detached.
 */
export function HeaderBlock({
  visibility = DEFAULT_HEADER_BLOCK_VISIBILITY,
  fitToWindow = false
}: HeaderBlockProps): React.JSX.Element {
  const boxRef = useRef<HTMLDivElement>(null)
  const contentRef = useRef<HTMLDivElement>(null)
  const { scale, wide } = useFitToBox(boxRef, contentRef, fitToWindow)

  return (
    <div className="header-center poppable" ref={boxRef}>
      <PopOutButton
        panelId="header-block"
        deckId={null}
        title="Open the lineup header in its own window"
      />
      {/* `.header-block-content` is `display: contents` until `.header-block-fit`
          joins it, so the app header's own layout sees no extra box. */}
      <div
        ref={contentRef}
        className={`header-block-content${fitToWindow ? ' header-block-fit' : ''}`}
        data-layout={wide ? 'row' : 'stacked'}
        style={fitToWindow ? { ['--fit-scale' as string]: String(scale) } : undefined}
      >
        <div className="header-clocks">
          {visibility.clock && <HeaderClock />}
          {visibility.timecode && (
            <>
              <TimecodeClock />
              <TimecodeSourceButtons />
            </>
          )}
        </div>
        {visibility.lineup && <LineupNow />}
      </div>
    </div>
  )
}
