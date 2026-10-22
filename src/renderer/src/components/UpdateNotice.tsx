import { updateToAnnounce } from '../lib/update-check'
import { dismissUpdate, useUpdateCheckStore } from '../state/update-check'
import { useAppVersion } from './useAppVersion'
import { useOpenProjectLink } from './useOpenProjectLink'

/**
 * A small corner card announcing a newer release, with a link to its
 * download page. Stays out of the way of the decks, and "Later" hides it
 * until the next release comes out.
 */
export function UpdateNotice(): React.JSX.Element | null {
  const currentVersion = useAppVersion()
  const latestVersion = useUpdateCheckStore((s) => s.latestVersion)
  const dismissedVersion = useUpdateCheckStore((s) => s.dismissedVersion)
  const { open, linkError } = useOpenProjectLink()

  const version = updateToAnnounce(latestVersion, currentVersion, dismissedVersion)
  if (version === null) return null

  return (
    <div className="update-notice" role="status">
      <p className="update-notice-text">Decktector {version} is available.</p>
      {linkError !== null && <p className="project-link-error">{linkError}</p>}
      <div className="update-notice-actions">
        <button type="button" className="setting-btn" onClick={() => open('latestRelease')}>
          Download
        </button>
        <button type="button" className="setting-btn" onClick={() => dismissUpdate(version)}>
          Later
        </button>
      </div>
    </div>
  )
}
