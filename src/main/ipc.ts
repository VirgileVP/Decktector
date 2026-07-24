import { BrowserWindow, ipcMain } from 'electron'

import { ENGINE_EVENTS, type EngineEvents, type ProlinkEngine } from '../engine/engine'

/**
 * Bridges the engine to the renderer: every engine event is re-emitted on
 * the `engine:<event>` IPC channel, and `engine:snapshot` lets a fresh
 * renderer hydrate its store.
 */
export function exposeEngineOverIpc(engine: ProlinkEngine): void {
  ipcMain.handle('engine:snapshot', () => engine.getSnapshot())

  const forward = <E extends keyof EngineEvents>(event: E): void => {
    engine.on(event, ((...args: EngineEvents[E]) => {
      for (const win of BrowserWindow.getAllWindows()) {
        win.webContents.send(`engine:${event}`, ...args)
      }
    }) as never)
  }

  for (const event of ENGINE_EVENTS) forward(event)
}
