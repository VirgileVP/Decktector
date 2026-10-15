import { useEffect, useId, useRef } from 'react'

import { AboutText, ProjectLinkButtons } from './AboutContent'
import { useOpenProjectLink } from './useOpenProjectLink'

interface WelcomeDialogProps {
  onClose: () => void
}

/**
 * First-launch notice: Decktector is free and open source, independent of
 * Pioneer DJ / AlphaTheta, and bug reports and contributions are welcome on
 * GitHub. Escape, a backdrop click and the dismiss button all close it.
 */
export function WelcomeDialog({ onClose }: WelcomeDialogProps): React.JSX.Element {
  const dismissRef = useRef<HTMLButtonElement>(null)
  const titleId = useId()
  const bodyId = useId()
  const { open, linkError } = useOpenProjectLink()

  useEffect(() => {
    dismissRef.current?.focus()
  }, [])

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
          <AboutText />
          {linkError !== null && <p className="project-link-error">{linkError}</p>}
        </div>
        <div className="confirm-actions">
          <ProjectLinkButtons onOpen={open} />
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
