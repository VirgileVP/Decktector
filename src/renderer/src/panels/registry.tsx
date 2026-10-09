/**
 * Maps each `PanelId` to what it renders and how big its window starts. This
 * is the seam that makes a composable mode possible later — adding a panel
 * is adding a row here, not touching every place that opens a window.
 */

import { DeckPanelHost } from '../components/DeckPanelHost'
import { HeaderBlock } from '../components/HeaderBlock'
import { HeaderBlockVisibilityToggle } from '../components/HeaderBlockVisibilityToggle'
import { HeaderClock } from '../components/HeaderClock'
import { TimecodeClock } from '../components/TimecodeClock'
import { TimecodeSourceButtons } from '../components/TimecodeSourceButtons'
import { DeckDisplayControl } from './DeckDisplayControl'
import type { PanelDisplayByPanel } from './panel-display'
import type { DisplayControlProps } from './PanelDisplayPopover'
import type { PanelId, PanelParams } from './params'

interface PanelSize {
  width: number
  height: number
}

const CLOCK_DEFAULT_SIZE: PanelSize = { width: 320, height: 140 }
const CLOCK_MIN_SIZE: PanelSize = { width: 160, height: 80 }

const TIMECODE_DEFAULT_SIZE: PanelSize = { width: 380, height: 160 }
const TIMECODE_MIN_SIZE: PanelSize = { width: 200, height: 90 }

const HEADER_BLOCK_DEFAULT_SIZE: PanelSize = { width: 640, height: 220 }
const HEADER_BLOCK_MIN_SIZE: PanelSize = { width: 320, height: 120 }

const DECK_DEFAULT_SIZE: PanelSize = { width: 1000, height: 340 }
const DECK_MIN_SIZE: PanelSize = { width: 420, height: 160 }

export interface PanelDefinition<P extends PanelId> {
  /** Window title, also shown in the chrome bar. */
  title: (params: PanelParams) => string
  defaultSize: PanelSize
  minSize: PanelSize
  render: (params: PanelParams, display: PanelDisplayByPanel[P]) => React.JSX.Element
  /** The chrome control that edits the window's display — omitted when the panel has nothing to choose. */
  DisplayControl?: React.ComponentType<DisplayControlProps<PanelDisplayByPanel[P]>>
}

/**
 * Mapped rather than a plain `Record` so that, indexed with a generic panel
 * id, each entry keeps the display type of its own panel — see
 * `PanelWindowContent`.
 */
export const PANELS: { [P in PanelId]: PanelDefinition<P> } = {
  clock: {
    title: () => 'Clock',
    defaultSize: CLOCK_DEFAULT_SIZE,
    minSize: CLOCK_MIN_SIZE,
    render: () => <HeaderClock />
  },
  timecode: {
    title: () => 'Timecode',
    defaultSize: TIMECODE_DEFAULT_SIZE,
    minSize: TIMECODE_MIN_SIZE,
    render: () => (
      <>
        <TimecodeClock />
        <TimecodeSourceButtons />
      </>
    )
  },
  'header-block': {
    title: () => 'Time',
    defaultSize: HEADER_BLOCK_DEFAULT_SIZE,
    minSize: HEADER_BLOCK_MIN_SIZE,
    render: (_params, display) => (
      <HeaderBlock visibility={display} lineupView={display.lineupView} fitToWindow />
    ),
    DisplayControl: HeaderBlockVisibilityToggle
  },
  deck: {
    title: (params) => `Deck ${params.deckId ?? '?'}`,
    defaultSize: DECK_DEFAULT_SIZE,
    minSize: DECK_MIN_SIZE,
    render: (params, display) =>
      params.deckId === null ? (
        <div className="panel-deck-placeholder">No deck selected</div>
      ) : (
        <DeckPanelHost deckId={params.deckId} view={display} />
      ),
    DisplayControl: DeckDisplayControl
  }
}
