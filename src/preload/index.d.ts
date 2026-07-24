import { ElectronAPI } from '@electron-toolkit/preload'

import type { DecktectiveApi } from './api'

declare global {
  interface Window {
    electron: ElectronAPI
    decktective: DecktectiveApi
  }
}
