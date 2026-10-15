import { useEffect, useId, useRef, useState } from 'react'

import { openProjectLink, type ProjectLink } from '../state/project-links'

interface WelcomeDialogProps {
  onClose: () => void
}

/**
 * First-launch notice: Decktector is free and open source, independent of
 * Pioneer DJ / AlphaTheta, and bug reports and contributions are welcome on
 * GitHub. Escape, a backdrop click and the
 * dismiss button all close it.
 */
export function WelcomeDialog({ onClose }: WelcomeDialogProps): React.JSX.Element {
  const dismissRef = useRef<HTMLButtonElement>(null)
  const titleId = useId()
  const bodyId = useId()
  const [linkError, setLinkError] = useState<string | null>(null)

  useEffect(() => {
    dismissRef.current?.focus()
  }, [])

  const open = (link: ProjectLink): void => {
    setLinkError(null)
    openProjectLink(link).catch((error: Error) => setLinkError(error.message))
  }

  const handleKeyDown = (event: React.KeyboardEvent): void => {
    if (event.key !== 'Escape') return
    event.stopPropagation()
    onClose()
  }

  return (
    <div className="confirm-backdrop" onClick={onClose}>
      <div
        className="confirm-panel welcome-panel"
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={bodyId}
        onClick={(event) => event.stopPropagation()}
        onKeyDown={handleKeyDown}
      >
        <h3 id={titleId} className="confirm-title welcome-title">
          Welcome to Decktector
        </h3>
        <div id={bodyId} className="confirm-body">
          <p>Decktector is free and open-source software, released under the GPL-3.0 license.</p>
          <p>
            It is an independent project, not affiliated with, endorsed by or sponsored by Pioneer
            DJ or AlphaTheta. Product names are trademarks of their respective owners.
          </p>
          <p>
            Found a bug, or have an idea? Please report it on GitHub — reports made with real
            CDJ/XDJ gear help the most. Contributions are welcome too.
          </p>
          {linkError !== null && <p className="welcome-error">{linkError}</p>}
        </div>
        <div className="confirm-actions">
          <button type="button" className="setting-btn" onClick={() => open('issues')}>
            Report a bug
          </button>
          <button type="button" className="setting-btn" onClick={() => open('repository')}>
            View on GitHub
          </button>
          <button
            ref={dismissRef}
            type="button"
            className="setting-btn welcome-dismiss"
            onClick={onClose}
          >
            Got it
          </button>
        </div>
      </div>
    </div>
  )
}
