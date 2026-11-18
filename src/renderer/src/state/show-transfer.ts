import { invoke } from '@tauri-apps/api/core'

import type { ShowData } from '../lib/show'
import { parseShowFile, serializeShow, showFileName, type ShowFileResult } from '../lib/show-file'
import { toError } from './tauri-error'

/**
 * Asks the user where to save the show and writes it there. Resolves to the
 * saved path, or null if the dialog was cancelled.
 */
export async function exportShowToFile(show: ShowData): Promise<string | null> {
  try {
    return await invoke<string | null>('export_show_file', {
      contents: serializeShow(show),
      suggestedName: showFileName(new Date())
    })
  } catch (reason) {
    throw toError(reason)
  }
}

/**
 * Asks the user for a show file and reads it. Resolves to null if the dialog
 * was cancelled; a file that is not a valid show comes back as a failed
 * result rather than an exception, since that is the user's mistake to
 * correct, not the app's.
 */
export async function importShowFromFile(): Promise<ShowFileResult | null> {
  let text: string | null
  try {
    text = await invoke<string | null>('import_show_file')
  } catch (reason) {
    throw toError(reason)
  }
  return text === null ? null : parseShowFile(text)
}
