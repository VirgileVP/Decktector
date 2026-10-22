import { isNewerVersion } from '../lib/update-check'
import { useUpdateCheckStore } from '../state/update-check'
import { AboutText, ProjectLinkButtons } from './AboutContent'
import { useAppVersion } from './useAppVersion'
import { useOpenProjectLink } from './useOpenProjectLink'

/** Settings › About: the welcome notice's content, reachable after it was dismissed. */
export function AboutSettings(): React.JSX.Element {
  const { open, linkError } = useOpenProjectLink()
  const version = useAppVersion()
  const latestVersion = useUpdateCheckStore((s) => s.latestVersion)
  // Unlike the corner notice, this ignores "Later": someone opening About
  // is looking for exactly this.
  const updateAvailable =
    version !== null && latestVersion !== null && isNewerVersion(latestVersion, version)

  return (
    <section className="settings-section">
      <h3>About Decktector</h3>
      {version !== null && (
        <p className="about-version setting-value-selectable">Version {version}</p>
      )}
      {updateAvailable && <p className="about-update">Version {latestVersion} is available.</p>}
      <div className="about-body">
        <AboutText />
        {linkError !== null && <p className="project-link-error">{linkError}</p>}
      </div>
      <div className="about-links">
        {updateAvailable && (
          <button type="button" className="setting-btn" onClick={() => open('latestRelease')}>
            Download {latestVersion}
          </button>
        )}
        <ProjectLinkButtons onOpen={open} />
      </div>
    </section>
  )
}
