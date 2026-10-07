import { useState } from 'react'

import { ConfirmDialog } from './ConfirmDialog'
import { ToggleRow } from './SettingControls'

interface TrackDownloadSettingProps {
  value: boolean
  onChange: (value: boolean) => void
}

/** Track download toggle. Turning it on goes through a warning first; turning it off does not. */
export function TrackDownloadSetting({
  value,
  onChange
}: TrackDownloadSettingProps): React.JSX.Element {
  const [confirming, setConfirming] = useState(false)

  const handleToggle = (checked: boolean): void => {
    if (checked) setConfirming(true)
    else onChange(false)
  }

  return (
    <>
      <ToggleRow
        label="Allow track downloads (experimental)"
        hint="Shows a discreet download button on each deck to fetch audio files directly from the player's USB drive over the network"
        checked={value}
        onChange={handleToggle}
      />
      {confirming && (
        <ConfirmDialog
          title="Enable track downloads?"
          confirmLabel="Enable downloads"
          onConfirm={() => {
            setConfirming(false)
            onChange(true)
          }}
          onCancel={() => setConfirming(false)}
        >
          <p>
            This feature is experimental. A download reads the file straight from the player&rsquo;s
            media over the Pro DJ Link network, which adds load on the network and on the player —
            avoid it while a set is running.
          </p>
          <p>
            Downloading a track without the rights holder&rsquo;s consent is considered theft. Only
            copy tracks you own or have the rights to. Files are saved to Downloads/Decktector.
          </p>
        </ConfirmDialog>
      )}
    </>
  )
}
