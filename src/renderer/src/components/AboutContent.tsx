import type { ProjectLink } from '../state/project-links'

/** What Decktector is and where to report bugs — shared by the welcome notice and Settings › About. */
export function AboutText(): React.JSX.Element {
  return (
    <>
      <p>Decktector is free and open-source software, released under the GPL-3.0 license.</p>
      <p>
        It is an independent project, not affiliated with, endorsed by or sponsored by Pioneer DJ or
        AlphaTheta. Product names are trademarks of their respective owners.
      </p>
      <p>
        Found a bug, or have an idea? Please report it on GitHub — reports made with real CDJ/XDJ
        gear help the most. Contributions are welcome too.
      </p>
    </>
  )
}

interface ProjectLinkButtonsProps {
  onOpen: (link: ProjectLink) => void
}

export function ProjectLinkButtons({ onOpen }: ProjectLinkButtonsProps): React.JSX.Element {
  return (
    <>
      <button type="button" className="setting-btn" onClick={() => onOpen('issues')}>
        Report a bug
      </button>
      <button type="button" className="setting-btn" onClick={() => onOpen('repository')}>
        View on GitHub
      </button>
    </>
  )
}
