/**
 * Maps each `PanelId` to what it renders and how big its window starts. This
 * is the seam that makes a composable mode possible later — adding a panel
 * is adding a row here, not touching every place that opens a window.
 */

import { DeckPanelHost } from '../components/DeckPanelHost'
import { HeaderBlock } from '../components/HeaderBlock'
import { HeaderClock } from '../components/HeaderClock'
import { TimecodeClock } from '../components/TimecodeClock'
import { TimecodeSourceButtons } from '../components/TimecodeSourceButtons'
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

export interface PanelDefinition {
  /** Window title, also shown in the chrome bar. */
  title: (params: PanelParams) => string
  defaultSize: PanelSize
  minSize: PanelSize
  /** Whether the panel offers the compact variant in its chrome. */
  hasCompactVariant: boolean
  /** Whether `PanelWindow` supplies a show/hide toggle for this panel's chrome — only `header-block` does. */
  hasVisibilityToggle: boolean
  render: (params: PanelParams, compact: boolean) => React.JSX.Element
}

export const PANELS: Record<PanelId, PanelDefinition> = {
  clock: {
    title: () => 'Clock',
    defaultSize: CLOCK_DEFAULT_SIZE,
    minSize: CLOCK_MIN_SIZE,
    hasCompactVariant: false,
    hasVisibilityToggle: false,
    render: () => <HeaderClock />
  },
  timecode: {
    title: () => 'Timecode',
    defaultSize: TIMECODE_DEFAULT_SIZE,
    minSize: TIMECODE_MIN_SIZE,
    hasCompactVariant: false,
    hasVisibilityToggle: false,
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
    hasCompactVariant: false,
    hasVisibilityToggle: true,
    // PanelWindow overrides this with the visibility-controlled version —
    // this default (show everything) only runs if that ever changes.
    render: () => <HeaderBlock />
  },
  deck: {
    title: (params) => `Deck ${params.deckId ?? '?'}`,
    defaultSize: DECK_DEFAULT_SIZE,
    minSize: DECK_MIN_SIZE,
    hasCompactVariant: true,
    hasVisibilityToggle: false,
    render: (params, compact) =>
      params.deckId === null ? (
        <div className="panel-deck-placeholder">No deck selected</div>
      ) : (
        <DeckPanelHost deckId={params.deckId} variant={compact ? 'compact' : 'full'} />
      )
  }
}
