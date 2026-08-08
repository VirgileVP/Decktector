import { useEffect, useState } from 'react'

import { HeaderBlock } from '../components/HeaderBlock'
import { HeaderBlockVisibilityToggle } from '../components/HeaderBlockVisibilityToggle'
import { DEFAULT_HEADER_BLOCK_VISIBILITY } from '../components/HeaderBlockVisibility'
import { connectDevicesToEngine } from '../state/devices'
import { currentPanelWindowRecord } from '../state/panel-windows'
import { connectSettingsBroadcast } from '../state/settings-broadcast'
import { connectSettingsToApp } from '../state/settings-sync'
import { connectStoreToEngine } from '../state/store'
import { connectTimecodeToEngine } from '../state/timecode'
import { PanelChrome } from './PanelChrome'
import { PANELS } from './registry'
import type { PanelParams } from './params'

interface PanelWindowProps {
  params: PanelParams
}

/**
 * Renders a single panel, mirroring a piece of the main window's UI in its
 * own OS window. Mounts the same engine connections `App` does, minus
 * `connectRemoteToEngine` — the remote link banner is the main window's
 * concern, not a panel's.
 */
export function PanelWindow({ params }: PanelWindowProps): React.JSX.Element {
  useEffect(() => connectStoreToEngine(), [])
  useEffect(() => connectSettingsToApp('panel'), [])
  useEffect(() => connectSettingsBroadcast(), [])
  useEffect(() => connectTimecodeToEngine(), [])
  useEffect(() => connectDevicesToEngine(), [])

  // Seeded from this window's own record — see `PanelChrome`'s matching
  // seed for its display options.
  const [compact, setCompact] = useState<boolean>(() => currentPanelWindowRecord()?.compact ?? false)

  // Only meaningful for `header-block` (`hasVisibilityToggle`), but a hook
  // cannot be called conditionally — it is simply unused for every other panel.
  const [headerBlockVisibility, setHeaderBlockVisibility] = useState(DEFAULT_HEADER_BLOCK_VISIBILITY)

  const definition = PANELS[params.panelId]

  // `header-block` bypasses the registry's default render: its content and
  // its toolbar control share one piece of state, which only this
  // component — the common parent of `PanelChrome` and the content it
  // wraps — can hold.
  const content = definition.hasVisibilityToggle ? (
    <HeaderBlock visibility={headerBlockVisibility} />
  ) : (
    definition.render(params, compact)
  )
  const toolbarExtra = definition.hasVisibilityToggle ? (
    <HeaderBlockVisibilityToggle visibility={headerBlockVisibility} onChange={setHeaderBlockVisibility} />
  ) : undefined

  return (
    <div className="panel-window">
      <PanelChrome
        title={definition.title(params)}
        params={params}
        hasCompactVariant={definition.hasCompactVariant}
        compact={compact}
        onToggleCompact={() => setCompact((current) => !current)}
        toolbarExtra={toolbarExtra}
      >
        {content}
      </PanelChrome>
    </div>
  )
}
