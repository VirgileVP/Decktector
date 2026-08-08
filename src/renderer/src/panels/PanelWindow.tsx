import { useEffect } from 'react'

import { connectDevicesToEngine } from '../state/devices'
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
  useEffect(() => connectTimecodeToEngine(), [])
  useEffect(() => connectDevicesToEngine(), [])

  // Hardcoded until task 6.1 adds the compact deck layout and its toggle.
  const compact = false
  const definition = PANELS[params.panelId]

  return (
    <div className="panel-window">
      <PanelChrome title={definition.title(params)}>{definition.render(params, compact)}</PanelChrome>
    </div>
  )
}
