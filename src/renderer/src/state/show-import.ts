import { invoke } from '@tauri-apps/api/core'

import type { OpenedRekordboxLibrary, RekordboxPlaylist } from '../model'
import { toError } from './tauri-error'

/**
 * Asks the user for a rekordbox XML export and reads it in the app shell.
 * Resolves to its playlist tree, or null if the dialog was cancelled.
 */
export async function openRekordboxXml(): Promise<OpenedRekordboxLibrary | null> {
  try {
    return await invoke<OpenedRekordboxLibrary | null>('open_rekordbox_xml')
  } catch (reason) {
    throw toError(reason)
  }
}

/** The tracks of the playlist at `path` (child indices from the root) of the export `token` names. */
export async function readRekordboxPlaylist(
  token: string,
  path: readonly number[]
): Promise<RekordboxPlaylist> {
  try {
    return await invoke<RekordboxPlaylist>('read_rekordbox_playlist', { token, path })
  } catch (reason) {
    throw toError(reason)
  }
}

/** Lets the app shell drop the export `token` names, once the import is done or cancelled. */
export function closeRekordboxXml(token: string): void {
  invoke('close_rekordbox_xml', { token }).catch((error) =>
    console.error('Failed to close the rekordbox export:', error)
  )
}
