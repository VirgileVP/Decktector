import type { BeatGridBeat, TrackInfo, WaveformData, WaveformSegment } from '../types'

/** Columns per second in the generated detail waveform (matches HD waveforms). */
const DETAIL_SEGMENTS_PER_SECOND = 150
const PREVIEW_SEGMENT_COUNT = 400
const BEATS_PER_BAR = 4
/** Bars per musical phrase used to shape the energy envelope. */
const BARS_PER_PHRASE = 16

export interface SimTrackSpec {
  title: string
  artist: string
  album: string
  genre: string
  key: string
  bpm: number
  durationSec: number
}

export const SIM_TRACKS: SimTrackSpec[] = [
  {
    title: 'Midnight Interrogation',
    artist: 'The Case Files',
    album: 'Undercover EP',
    genre: 'Techno',
    key: '8A',
    bpm: 128,
    durationSec: 372
  },
  {
    title: 'Smoking Gun',
    artist: 'Magnifying Bass',
    album: 'Prime Suspects',
    genre: 'House',
    key: '5A',
    bpm: 124,
    durationSec: 341
  },
  {
    title: 'Alibi (Extended Mix)',
    artist: 'Deck Sherlock',
    album: 'Cold Cases',
    genre: 'Melodic Techno',
    key: '11B',
    bpm: 122,
    durationSec: 415
  },
  {
    title: 'Stakeout Groove',
    artist: 'The Case Files',
    album: 'Undercover EP',
    genre: 'Tech House',
    key: '2A',
    bpm: 126,
    durationSec: 358
  }
]

/** Deterministic pseudo-random generator so waveforms are stable per track. */
function mulberry32(seed: number): () => number {
  let a = seed
  return () => {
    a |= 0
    a = (a + 0x6d2b79f5) | 0
    let t = Math.imul(a ^ (a >>> 15), 1 | a)
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296
  }
}

/**
 * Energy envelope over the track: intro and outro stay low, phrases
 * alternate between builds and drops in between.
 */
function phraseEnergy(beatIndex: number, totalBeats: number): number {
  const progress = beatIndex / totalBeats
  if (progress < 0.06) return 0.35
  if (progress > 0.94) return 0.3
  const phrase = Math.floor(beatIndex / (BEATS_PER_BAR * BARS_PER_PHRASE))
  const inBreakdown = phrase % 4 === 2
  if (inBreakdown) {
    const phraseProgress =
      (beatIndex % (BEATS_PER_BAR * BARS_PER_PHRASE)) / (BEATS_PER_BAR * BARS_PER_PHRASE)
    return 0.25 + 0.55 * phraseProgress
  }
  return 0.9
}

function buildBeatGrid(bpm: number, durationSec: number): BeatGridBeat[] {
  const beatMs = 60000 / bpm
  const beats: BeatGridBeat[] = []
  for (let ms = 0, i = 0; ms < durationSec * 1000; ms += beatMs, i++) {
    beats.push({ offsetMs: ms, beatInMeasure: (i % BEATS_PER_BAR) + 1 })
  }
  return beats
}

function buildWaveform(spec: SimTrackSpec, seed: number): WaveformData {
  const random = mulberry32(seed)
  const totalSegments = Math.floor(spec.durationSec * DETAIL_SEGMENTS_PER_SECOND)
  const segmentsPerBeat = (DETAIL_SEGMENTS_PER_SECOND * 60) / spec.bpm
  const totalBeats = Math.ceil(totalSegments / segmentsPerBeat)

  const detail: WaveformSegment[] = new Array(totalSegments)
  for (let i = 0; i < totalSegments; i++) {
    const beatIndex = Math.floor(i / segmentsPerBeat)
    const beatPhase = (i % segmentsPerBeat) / segmentsPerBeat
    const energy = phraseEnergy(beatIndex, totalBeats)
    // Kick transient at the start of each beat, decaying across it
    const transient = Math.exp(-beatPhase * 5)
    const noise = random() * 0.08
    const height = Math.min(1, 0.12 + energy * (0.35 + 0.55 * transient) + noise)
    // Bass-heavy columns render deep blue, brighter ones toward white
    const brightness = Math.min(1, energy * transient + noise)
    const color: [number, number, number] = [
      Math.round(40 + 180 * brightness),
      Math.round(90 + 130 * brightness),
      255
    ]
    detail[i] = { height, color }
  }

  const preview: WaveformSegment[] = new Array(PREVIEW_SEGMENT_COUNT)
  const chunk = totalSegments / PREVIEW_SEGMENT_COUNT
  for (let i = 0; i < PREVIEW_SEGMENT_COUNT; i++) {
    const start = Math.floor(i * chunk)
    const end = Math.min(totalSegments, Math.ceil((i + 1) * chunk))
    let maxHeight = 0
    let sampleColor: [number, number, number] = [40, 90, 255]
    for (let j = start; j < end; j++) {
      if (detail[j].height > maxHeight) {
        maxHeight = detail[j].height
        sampleColor = detail[j].color
      }
    }
    preview[i] = { height: maxHeight, color: sampleColor }
  }

  return {
    detail,
    msPerDetailSegment: 1000 / DETAIL_SEGMENTS_PER_SECOND,
    preview
  }
}

export function buildSimTrack(spec: SimTrackSpec, trackId: number): TrackInfo {
  return {
    trackId,
    title: spec.title,
    artist: spec.artist,
    album: spec.album,
    genre: spec.genre,
    key: spec.key,
    label: 'Decktective Records',
    comment: null,
    durationMs: spec.durationSec * 1000,
    bpm: spec.bpm,
    artwork: null,
    beatGrid: buildBeatGrid(spec.bpm, spec.durationSec),
    waveform: buildWaveform(spec, trackId * 7919)
  }
}
