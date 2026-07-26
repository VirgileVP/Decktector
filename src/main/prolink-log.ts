import { appendFile } from 'fs/promises'
import { join } from 'path'

import { setDiagnosticSink } from '../engine/prolink/diagnostics'
import { resolveLogDir } from './log-dir'

/**
 * Persists the Pro DJ Link trace to a file, so a metadata problem that only
 * shows up on real hardware can be read back after the set instead of having
 * to be reproduced in front of the players.
 *
 * Appends are fire-and-forget and ordered by the queue below: the trace is
 * low volume (a few lines per track load), and losing a line must never
 * disturb the network handling.
 */
export function attachDiagnosticLog(): string {
  const logPath = join(resolveLogDir(), 'prolink-diagnostic.log')
  let queue: Promise<void> = Promise.resolve()

  setDiagnosticSink((line) => {
    queue = queue
      .then(() => appendFile(logPath, `${line}\n`, 'utf8'))
      .catch((error) => console.error('Failed to write Pro DJ Link diagnostic log:', error))
  })

  return logPath
}
