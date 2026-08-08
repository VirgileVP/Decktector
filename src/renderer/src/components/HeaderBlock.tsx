import { HeaderClock } from './HeaderClock'
import { LineupNow } from './LineupNow'
import { PopOutButton } from './PopOutButton'
import { TimecodeClock } from './TimecodeClock'
import { TimecodeSourceButtons } from './TimecodeSourceButtons'

/**
 * The clocks-plus-lineup block shown in the header — also popped out as its
 * own panel window. The clock and the timecode have no pop-out of their
 * own: only this combined window can be detached, and the window it opens
 * will get its own show/hide toggles for each piece (see `PanelChrome`'s
 * compact/visibility controls).
 */
export function HeaderBlock(): React.JSX.Element {
  return (
    <div className="header-center poppable">
      <PopOutButton panelId="header-block" deckId={null} title="Open the lineup header in its own window" />
      <div className="header-clocks">
        <HeaderClock />
        <TimecodeClock />
        <TimecodeSourceButtons />
      </div>
      <LineupNow />
    </div>
  )
}
