import { useState } from 'react'

import { ConfirmDialog } from './ConfirmDialog'

interface ResetSettingsSettingProps {
  onReset: () => void
}

/** Puts every setting back to its default, behind a confirmation — nothing here can be undone. */
export function ResetSettingsSetting({ onReset }: ResetSettingsSettingProps): React.JSX.Element {
  const [confirming, setConfirming] = useState(false)

  return (
    <>
      <div className="setting-row">
        <span className="setting-text">
          <span className="setting-label">Reset all settings</span>
          <span className="setting-hint">
            Puts every setting in every tab back to its default. The lineup is kept.
          </span>
        </span>
        <button type="button" className="setting-btn" onClick={() => setConfirming(true)}>
          Reset…
        </button>
      </div>
      {confirming && (
        <ConfirmDialog
          title="Reset all settings?"
          confirmLabel="Reset settings"
          onConfirm={() => {
            setConfirming(false)
            onReset()
          }}
          onCancel={() => setConfirming(false)}
        >
          <p>
            Theme, deck view, deck labels, timecode, remote view and network choices all go back to
            their defaults. This cannot be undone. The lineup is not touched.
          </p>
        </ConfirmDialog>
      )}
    </>
  )
}
