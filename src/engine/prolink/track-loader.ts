import type { CDJStatus, ConnectedProlinkNetwork } from 'prolink-connect'

import type { TrackInfo, WaveformData } from '../types'
import { buildPreviewFromDetail } from '../waveform'
import { mapCues } from './cues'

/** HD waveforms carry 150 columns per second of audio. */
const HD_SEGMENTS_PER_SECOND = 150
/** Height of a raw waveform column (0-31). */
const MAX_SEGMENT_HEIGHT = 31

type RawWaveformHd = { height: number; color: [number, number, number] }[]

function convertWaveform(waveformHd: RawWaveformHd | null): WaveformData | null {
  if (!waveformHd || waveformHd.length === 0) return null

  const detail = waveformHd.map((segment) => ({
    height: segment.height / MAX_SEGMENT_HEIGHT,
    // prolink-connect color channels are 0-1 floats
    color: [
      Math.round(segment.color[0] * 255),
      Math.round(segment.color[1] * 255),
      Math.round(segment.color[2] * 255)
    ] as [number, number, number]
  }))

  return {
    detail,
    msPerDetailSegment: 1000 / HD_SEGMENTS_PER_SECOND,
    preview: buildPreviewFromDetail(detail)
  }
}

/**
 * Rekordbox databases store tempo as BPM×100 in some code paths; normalize
 * to actual BPM either way.
 */
function normalizeTempo(tempo: number): number | null {
  if (!tempo || tempo <= 0) return null
  return tempo > 400 ? tempo / 100 : tempo
}

/**
 * Fetches everything about the track referenced by a status packet:
 * metadata, artwork and waveforms, through whichever database strategy
 * (localdb over NFS or remotedb) prolink-connect deems available.
 */
export async function loadTrack(
  network: ConnectedProlinkNetwork,
  status: CDJStatus.State
): Promise<TrackInfo | null> {
  const query = {
    deviceId: status.trackDeviceId,
    trackSlot: status.trackSlot as never,
    trackType: status.trackType as never,
    trackId: status.trackId
  }

  const track = await network.db.getMetadata(query)
  if (!track) return null

  const [artwork, waveforms] = await Promise.all([
    network.db.getArtwork({ ...query, track }).catch((error) => {
      console.warn(`Artwork fetch failed for track ${track.id}:`, error)
      return null
    }),
    network.db.getWaveforms({ ...query, track }).catch((error) => {
      console.warn(`Waveform fetch failed for track ${track.id}:`, error)
      return null
    })
  ])

  return {
    trackId: track.id,
    title: track.title,
    artist: track.artist?.name ?? null,
    album: track.album?.name ?? null,
    genre: track.genre?.name ?? null,
    key: track.key?.name ?? null,
    label: track.label?.name ?? null,
    comment: track.comment || null,
    durationMs: track.duration * 1000,
    bpm: normalizeTempo(track.tempo) ?? status.trackBPM,
    artwork: artwork ? new Uint8Array(artwork) : null,
    beatGrid:
      track.beatGrid?.map((beat) => ({
        offsetMs: beat.offset,
        beatInMeasure: beat.count
      })) ?? null,
    waveform: convertWaveform((waveforms?.waveformHd ?? track.waveformHd) as RawWaveformHd | null),
    cues: mapCues(track.cueAndLoops)
  }
}
