import { useState } from 'react'

import { openProjectLink, type ProjectLink } from '../state/project-links'

/**
 * Opens project pages in the system browser and keeps the last failure's
 * message for display, cleared on the next attempt.
 */
export function useOpenProjectLink(): {
  open: (link: ProjectLink) => void
  linkError: string | null
} {
  const [linkError, setLinkError] = useState<string | null>(null)

  const open = (link: ProjectLink): void => {
    setLinkError(null)
    openProjectLink(link).catch((error: Error) => setLinkError(error.message))
  }

  return { open, linkError }
}
