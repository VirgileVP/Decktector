import { app } from 'electron'
import { is } from '@electron-toolkit/utils'
import { mkdirSync } from 'fs'
import { join } from 'path'

/**
 * Where log files live: an explicit DECKTECTIVE_LOG_DIR wins, dev runs log
 * into ./logs next to the project for easy access, and packaged builds use
 * the app's user-data directory.
 */
export function resolveLogDir(): string {
  const dir =
    process.env.DECKTECTIVE_LOG_DIR ??
    (is.dev ? join(process.cwd(), 'logs') : app.getPath('userData'))
  mkdirSync(dir, { recursive: true })
  return dir
}
