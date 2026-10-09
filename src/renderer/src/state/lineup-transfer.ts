import { invoke } from '@tauri-apps/api/core'

import type { LineupData } from '../lib/lineup'
import {
  lineupFileName,
  parseLineupFile,
  serializeLineup,
  type LineupFileResult
} from '../lib/lineup-file'

/** Tauri commands reject with the Rust side's error string; this gives callers an `Error`. */
function toError(reason: unknown): Error {
  return reason instanceof Error ? reason : new Error(String(reason))
}

/**
 * Asks the user where to save the lineup and writes it there. Resolves to the
 * saved path, or null if the dialog was cancelled.
 */
export async function exportLineupToFile(lineup: LineupData): Promise<string | null> {
  try {
    return await invoke<string | null>('export_lineup_file', {
      contents: serializeLineup(lineup),
      suggestedName: lineupFileName(new Date())
    })
  } catch (reason) {
    throw toError(reason)
  }
}

/**
 * Asks the user for a lineup file and reads it. Resolves to null if the
 * dialog was cancelled; a file that is not a valid lineup comes back as a
 * failed result rather than an exception, since that is the user's mistake to
 * correct, not the app's.
 */
export async function importLineupFromFile(): Promise<LineupFileResult | null> {
  let text: string | null
  try {
    text = await invoke<string | null>('import_lineup_file')
  } catch (reason) {
    throw toError(reason)
  }
  return text === null ? null : parseLineupFile(text)
}
