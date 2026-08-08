import { useState } from 'react'

import { DEFAULT_HEADER_BLOCK_VISIBILITY } from './HeaderBlockVisibility'
import { HeaderBlockVisibilitySettings } from './HeaderBlockVisibilitySettings'
import { HeaderClock } from './HeaderClock'
import { LineupNow } from './LineupNow'
import { PopOutButton } from './PopOutButton'
import { TimecodeClock } from './TimecodeClock'
import { TimecodeSourceButtons } from './TimecodeSourceButtons'

/**
 * The clocks-plus-lineup block shown in the header — also popped out as its
 * own panel window. The clock and the timecode have no pop-out of their
 * own: only this combined window can be detached. Once detached, its own
 * settings button (`HeaderBlockVisibilitySettings`) can show or hide each
 * piece independently — the main header always shows all three.
 */
export function HeaderBlock(): React.JSX.Element {
  const [visibility, setVisibility] = useState(DEFAULT_HEADER_BLOCK_VISIBILITY)

  return (
    <div className="header-center poppable">
      <PopOutButton panelId="header-block" deckId={null} title="Open the lineup header in its own window" />
      <HeaderBlockVisibilitySettings visibility={visibility} onChange={setVisibility} />
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
