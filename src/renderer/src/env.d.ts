/// <reference types="vite/client" />

interface ImportMetaEnv {
  /**
   * Overrides the remote WebSocket port during `vite dev`, where the page
   * (Vite's dev server) and the remote server run on different ports. Unset
   * in a packaged build, where both are served from the same port.
   */
  readonly VITE_REMOTE_WS_PORT?: string
}
