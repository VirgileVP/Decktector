import { HeaderClock } from './HeaderClock'
import { LineupNow } from './LineupNow'
import { TimecodeClock } from './TimecodeClock'
import { TimecodeSourceButtons } from './TimecodeSourceButtons'

/** The clocks-plus-lineup block shown in the header — also popped out as its own panel window. */
export function HeaderBlock(): React.JSX.Element {
  return (
    <div className="header-center">
      <div className="header-clocks">
        <HeaderClock />
        <TimecodeClock />
        <TimecodeSourceButtons />
      </div>
      <LineupNow />
    </div>
  )
}
