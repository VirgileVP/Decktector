import { MAX_CACHED_BUFFERS } from './audio-constants'

export type DecodeAudio = (data: ArrayBuffer) => Promise<AudioBuffer>

export interface BufferLoader {
  /** Fetches and decodes `url`; concurrent and repeated loads of one URL share the work. */
  load: (url: string) => Promise<AudioBuffer>
}

/**
 * Loads decoded audio by URL with a small least-recently-used cache: a track
 * on two decks is decoded once, but the cache cannot grow without bound since
 * decoded buffers are large. Failures are not cached.
 */
export function createBufferLoader(
  decode: DecodeAudio,
  fetchAudio: typeof fetch = fetch,
  maxEntries: number = MAX_CACHED_BUFFERS
): BufferLoader {
  const cache = new Map<string, Promise<AudioBuffer>>()

  const fetchAndDecode = async (url: string): Promise<AudioBuffer> => {
    const response = await fetchAudio(url)
    if (!response.ok) throw new Error(`GET ${url} answered ${response.status}`)
    return decode(await response.arrayBuffer())
  }

  return {
    load: (url) => {
      const cached = cache.get(url)
      if (cached !== undefined) {
        // Re-insert so Map order tracks recency.
        cache.delete(url)
        cache.set(url, cached)
        return cached
      }

      const pending = fetchAndDecode(url)
      cache.set(url, pending)
      pending.catch(() => {
        if (cache.get(url) === pending) cache.delete(url)
      })
      while (cache.size > maxEntries) {
        const oldest = cache.keys().next().value
        if (oldest === undefined) break
        cache.delete(oldest)
      }
      return pending
    }
  }
}
