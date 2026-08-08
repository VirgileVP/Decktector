import { HeaderClock } from './HeaderClock'
import { LineupNow } from './LineupNow'
import { PopOutButton } from './PopOutButton'
import { TimecodeClock } from './TimecodeClock'
import { TimecodeSourceButtons } from './TimecodeSourceButtons'

/** The clocks-plus-lineup block shown in the header — also popped out as its own panel window. */
export function HeaderBlock(): React.JSX.Element {
  return (
    <div className="header-center poppable">
      <PopOutButton panelId="header-block" deckId={null} title="Open the lineup header in its own window" />
      <div className="header-clocks">
        <span className="poppable">
          <HeaderClock />
          <PopOutButton panelId="clock" deckId={null} title="Open the clock in its own window" />
        </span>
        <span className="poppable">
          <TimecodeClock />
          <PopOutButton panelId="timecode" deckId={null} title="Open the timecode in its own window" />
        </span>
        <TimecodeSourceButtons />
      </div>
      <LineupNow />
    </div>
  )
}
