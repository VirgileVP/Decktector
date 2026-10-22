import { invoke } from '@tauri-apps/api/core'

/** Public project pages the shell knows how to open; the URLs live on the Rust side. */
export type ProjectLink = 'repository' | 'issues' | 'latestRelease'

/** Opens `link` in the system browser. Rejects with the shell's error message. */
export async function openProjectLink(link: ProjectLink): Promise<void> {
  try {
    await invoke('open_project_link', { link })
  } catch (reason) {
    throw reason instanceof Error ? reason : new Error(String(reason))
  }
}
