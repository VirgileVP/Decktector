import { AboutText, ProjectLinkButtons } from './AboutContent'
import { useOpenProjectLink } from './useOpenProjectLink'

/** Settings › About: the welcome notice's content, reachable after it was dismissed. */
export function AboutSettings(): React.JSX.Element {
  const { open, linkError } = useOpenProjectLink()

  return (
    <section className="settings-section">
      <h3>About Decktector</h3>
      <div className="about-body">
        <AboutText />
        {linkError !== null && <p className="project-link-error">{linkError}</p>}
      </div>
      <div className="about-links">
        <ProjectLinkButtons onOpen={open} />
      </div>
    </section>
  )
}
