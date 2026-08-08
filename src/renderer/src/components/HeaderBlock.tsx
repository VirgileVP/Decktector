import { DEFAULT_HEADER_BLOCK_VISIBILITY, type HeaderBlockVisibility } from './HeaderBlockVisibility'
import { HeaderClock } from './HeaderClock'
import { LineupNow } from './LineupNow'
import { PopOutButton } from './PopOutButton'
import { TimecodeClock } from './TimecodeClock'
import { TimecodeSourceButtons } from './TimecodeSourceButtons'

interface HeaderBlockProps {
  /** Defaults to showing everything — the main header always does. A panel
   * window controls this through its own chrome (`HeaderBlockVisibilityToggle`). */
  visibility?: HeaderBlockVisibility
}

/**
 * The clocks-plus-lineup block shown in the header — also popped out as its
 * own panel window (titled "Time" there). The clock and the timecode have
 * no pop-out of their own: only this combined window can be detached.
 */
export function HeaderBlock({
  visibility = DEFAULT_HEADER_BLOCK_VISIBILITY
}: HeaderBlockProps): React.JSX.Element {
  return (
    <div className="header-center poppable">
      <PopOutButton panelId="header-block" deckId={null} title="Open the lineup header in its own window" />
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
  )
}
