/**
 * URL rules for the remote server, stated once here so they are testable and
 * shared between the transport and anything else that needs to reach it
 * (e.g. a future "copy link" button).
 */

/** Query string key the remote server's URL carries the access token under. */
const TOKEN_PARAM = 'token'

/** Query string key for focusing a single deck full-screen. */
const DECK_PARAM = 'deck'

/**
 * The host and port the remote server listens on, from the page's own
 * location.
 *
 * In a packaged build the page and the server are served from the same
 * origin, so `location.port` is correct as-is. Under `vite dev`, the page is
 * served by Vite on its own port while the remote server listens on a
 * different one — `VITE_REMOTE_WS_PORT` overrides the port for that case
 * (it names the WebSocket, but the HTTP endpoints below live on the same
 * server and so share the override).
 */
function remoteAuthority(location: Location): string {
  const port = import.meta.env.VITE_REMOTE_WS_PORT ?? location.port
  return port ? `${location.hostname}:${port}` : location.hostname
}

/** The access token carried in the page's own query string, if any. */
export function remoteToken(location: Location): string | null {
  return new URLSearchParams(location.search).get(TOKEN_PARAM)
}

/**
 * Builds the `ws://`/`wss://` URL for the remote WebSocket. The token
 * travels once, in the page's query string, and the socket reuses it — the
 * shared link only has to carry it in one place.
 */
export function remoteSocketUrl(location: Location): string {
  const protocol = location.protocol === 'https:' ? 'wss:' : 'ws:'
  const token = remoteToken(location)
  const query = token ? `?${TOKEN_PARAM}=${encodeURIComponent(token)}` : ''
  return `${protocol}//${remoteAuthority(location)}/ws${query}`
}

/**
 * Builds the `http://`/`https://` URL for one of the remote server's plain
 * HTTP resources (currently just a track's artwork or waveform) — same
 * host/port rules as [`remoteSocketUrl`], with the token appended the same
 * way, since every remote route sits behind the one gate.
 */
export function remoteResourceUrl(location: Location, path: string): string {
  const protocol = location.protocol === 'https:' ? 'https:' : 'http:'
  const token = remoteToken(location)
  const query = token ? `?${TOKEN_PARAM}=${encodeURIComponent(token)}` : ''
  return `${protocol}//${remoteAuthority(location)}${path}${query}`
}

/**
 * The deck to show full-screen, from `?deck=N` in the page's own query
 * string — for a phone propped next to a lighting desk, where the whole
 * grid is more than is needed. `null` means show every connected deck, the
 * normal behaviour.
 */
export function remoteFocusedDeck(location: Location): number | null {
  const raw = new URLSearchParams(location.search).get(DECK_PARAM)
  if (raw === null) return null
  const deckId = Number(raw)
  return Number.isInteger(deckId) && deckId > 0 ? deckId : null
}
