import type { DeckTrack, EngineSnapshot, TrackInfo, WaveformData } from '../../model'
import { remoteResourceUrl } from '../../lib/remote-url'

/**
 * Fetches the artwork and waveform the remote server stripped off `track`
 * before sending it (see `wire::strip_heavy_payload` on the Rust side), and
 * merges them back in.
 *
 * A failed fetch degrades to a track missing that one piece, never to a
 * rejected promise — a slow or flaky network must not turn into a broken
 * deck, just a deck without a picture or a waveform.
 */
export async function rehydrateTrack(track: TrackInfo | null): Promise<TrackInfo | null> {
  if (!track) return track
  const [artwork, waveform] = await Promise.all([
    fetchArtwork(track.trackId),
    fetchWaveform(track.trackId)
  ])
  return { ...track, artwork, waveform }
}

/** Rehydrates every loaded deck's track in a snapshot the same way. */
export async function rehydrateSnapshotTracks(snapshot: EngineSnapshot): Promise<DeckTrack[]> {
  return Promise.all(
    snapshot.tracks.map(async (deckTrack) => ({
      ...deckTrack,
      track: await rehydrateTrack(deckTrack.track)
    }))
  )
}

async function fetchArtwork(trackId: number): Promise<string | null> {
  try {
    const response = await fetch(remoteResourceUrl(window.location, `/track/${trackId}/artwork`))
    if (!response.ok) return null
    return await blobToDataUrl(await response.blob())
  } catch (error) {
    console.error('Failed to fetch track artwork:', error)
    return null
  }
}

async function fetchWaveform(trackId: number): Promise<WaveformData | null> {
  try {
    const response = await fetch(remoteResourceUrl(window.location, `/track/${trackId}/waveform`))
    if (!response.ok) return null
    return (await response.json()) as WaveformData
  } catch (error) {
    console.error('Failed to fetch track waveform:', error)
    return null
  }
}

/** `TrackInfo.artwork` is documented as a ready-to-use `data:` URL — kept
 *  true here too, rather than switching to a `blob:` object URL, so every
 *  transport hands the store the exact same shape. */
function blobToDataUrl(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader()
    reader.onload = () => resolve(reader.result as string)
    reader.onerror = () => reject(reader.error ?? new Error('failed to read the artwork blob'))
    reader.readAsDataURL(blob)
  })
}
