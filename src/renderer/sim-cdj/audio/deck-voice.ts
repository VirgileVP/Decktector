import {
  DRIFT_RESEEK_THRESHOLD_MS,
  FADER_TIME_CONSTANT_SECONDS,
  MS_PER_SECOND
} from './audio-constants'
import { driftMs, type PlaybackTarget } from './follow-math'
import { playGrain } from './grain-player'
import { Playback } from './playback'
import { scratchGrain, type ScratchSample } from './scratch-math'

/**
 * The audio of one deck: its buffer, a channel gain driven by the mixer fader,
 * and whatever is playing (a followed {@link Playback} or scratch grains).
 */
export class DeckVoice {
  private readonly channelGain: GainNode
  private buffer: AudioBuffer | null = null
  private playback: Playback | null = null
  private lastScratchSample: ScratchSample | null = null

  constructor(
    private readonly context: AudioContext,
    master: AudioNode
  ) {
    this.channelGain = context.createGain()
    this.channelGain.connect(master)
  }

  /** Swaps the deck's audio; null (no track, or still loading) silences it. */
  setBuffer(buffer: AudioBuffer | null): void {
    this.silence()
    this.buffer = buffer
  }

  /** Channel fader level, 0..1. */
  setLevel(level: number): void {
    this.channelGain.gain.setTargetAtTime(
      level,
      this.context.currentTime,
      FADER_TIME_CONSTANT_SECONDS
    )
  }

  follow(target: PlaybackTarget): void {
    if (this.buffer === null || target.kind === 'silent') {
      this.silence()
      return
    }
    if (target.kind === 'scratch') {
      this.scratch(this.buffer, target.positionMs, target.atMs)
      return
    }
    this.lastScratchSample = null
    this.play(this.buffer, target)
  }

  silence(): void {
    this.releasePlayback()
    this.lastScratchSample = null
  }

  private play(buffer: AudioBuffer, target: Extract<PlaybackTarget, { kind: 'play' }>): void {
    const drifted =
      this.playback !== null &&
      Math.abs(driftMs(target.positionMs, this.playback.positionMs(), target.loop)) >
        DRIFT_RESEEK_THRESHOLD_MS
    if (this.playback !== null && !drifted) {
      this.playback.update(target.rate, target.loop)
      return
    }

    this.releasePlayback()
    // Past the end of the audio (e.g. the file is shorter than the metadata says).
    if (target.positionMs / MS_PER_SECOND >= buffer.duration) return
    this.playback = new Playback(
      this.context,
      buffer,
      this.channelGain,
      target.positionMs,
      target.rate,
      target.loop
    )
  }

  private scratch(buffer: AudioBuffer, positionMs: number, atMs: number): void {
    this.releasePlayback()
    const sample = { positionMs, atMs }
    const grain = scratchGrain(this.lastScratchSample, sample)
    this.lastScratchSample = sample
    if (grain !== null) playGrain(this.context, buffer, this.channelGain, grain)
  }

  private releasePlayback(): void {
    this.playback?.release()
    this.playback = null
  }
}
