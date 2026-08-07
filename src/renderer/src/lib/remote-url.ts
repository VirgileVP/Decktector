/**
 * URL rules for the remote WebSocket, stated once here so they are testable
 * and shared between the transport and anything else that needs to reach
 * the remote server (e.g. a future "copy link" button).
 */

/** Query string key the remote server's URL carries the access token under. */
const TOKEN_PARAM = 'token'

/**
 * Builds the `ws://`/`wss://` URL for the remote WebSocket, from the page's
 * own location. The token travels once, in the page's query string, and the
 * socket reuses it — the shared link only has to carry it in one place.
 *
 * In a packaged build the page and the socket are served from the same
 * origin, so `location.port` is correct as-is. Under `vite dev`, the page is
 * served by Vite on its own port while the remote server listens on a
 * different one — `VITE_REMOTE_WS_PORT` overrides the port for that case.
 */
export function remoteSocketUrl(location: Location): string {
  const protocol = location.protocol === 'https:' ? 'wss:' : 'ws:'
  const port = import.meta.env.VITE_REMOTE_WS_PORT ?? location.port
  const host = port ? `${location.hostname}:${port}` : location.hostname
  const token = new URLSearchParams(location.search).get(TOKEN_PARAM)
  const query = token ? `?${TOKEN_PARAM}=${encodeURIComponent(token)}` : ''
  return `${protocol}//${host}/ws${query}`
}
