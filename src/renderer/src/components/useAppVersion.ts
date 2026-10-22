import { getVersion } from '@tauri-apps/api/app'
import { isTauri } from '@tauri-apps/api/core'
import { useEffect, useState } from 'react'

/** The running app's version, as the shell reports it; `null` until known or outside Tauri. */
export function useAppVersion(): string | null {
  const [version, setVersion] = useState<string | null>(null)

  useEffect(() => {
    if (!isTauri()) return
    getVersion()
      .then(setVersion)
      .catch((error) => console.error('Failed to read the app version:', error))
  }, [])

  return version
}
