import { useEffect, useState } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'

import { connectDevicesToEngine } from '../state/devices'
import { panelWindowRecordFor } from '../state/panel-windows'
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
  const [compact, setCompact] = useState<boolean>(
    () => panelWindowRecordFor(getCurrentWindow().label)?.compact ?? false
  )

  const definition = PANELS[params.panelId]

  return (
    <div className="panel-window">
      <PanelChrome
        title={definition.title(params)}
        params={params}
        hasCompactVariant={definition.hasCompactVariant}
        compact={compact}
        onToggleCompact={() => setCompact((current) => !current)}
      >
        {definition.render(params, compact)}
      </PanelChrome>
    </div>
  )
}
