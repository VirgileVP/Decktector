import { useEffect, useState } from 'react'

import { connectDevicesToEngine } from '../state/devices'
import { connectLineupBroadcast } from '../state/lineup-broadcast'
import { currentPanelWindowRecord } from '../state/panel-windows'
import { pickDeckViewToggles, useSettings } from '../state/settings'
import { connectSettingsBroadcast } from '../state/settings-broadcast'
import { connectSettingsToApp } from '../state/settings-sync'
import { connectStoreToEngine } from '../state/store'
import { connectTimecodeToEngine } from '../state/timecode'
import { resolvePanelDisplay } from './panel-display'
import { PanelChrome } from './PanelChrome'
import { PANELS, type PanelDefinition } from './registry'
import type { PanelId, PanelParams } from './params'

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
  useEffect(() => connectLineupBroadcast(), [])
  useEffect(() => connectTimecodeToEngine(), [])
  useEffect(() => connectDevicesToEngine(), [])

  return <PanelWindowContent params={params} panelId={params.panelId} />
}

interface PanelWindowContentProps<P extends PanelId> {
  params: PanelParams
  /** `params.panelId`, passed on its own so `P` can tie the panel's definition to its display. */
  panelId: P
}

/**
 * The window's chrome and content. Holds the window's display — the common
 * parent of the chrome control that edits it and of the content it shapes.
 */
function PanelWindowContent<P extends PanelId>({
  params,
  panelId
}: PanelWindowContentProps<P>): React.JSX.Element {
  const definition: PanelDefinition<P> = PANELS[panelId]

  // Seeded from this window's own record — see `PanelChrome`'s matching
  // seed for its window options. A window with no usable record (or none at
  // all, outside Tauri) starts the way a fresh pop-out does.
  const [display, setDisplay] = useState(() =>
    resolvePanelDisplay(
      panelId,
      currentPanelWindowRecord()?.display,
      pickDeckViewToggles(useSettings.getState())
    )
  )

  const { DisplayControl } = definition

  return (
    <div className="panel-window">
      <PanelChrome
        title={definition.title(params)}
        params={params}
        display={display}
        toolbarExtra={DisplayControl && <DisplayControl display={display} onChange={setDisplay} />}
      >
        {definition.render(params, display)}
      </PanelChrome>
    </div>
  )
}
