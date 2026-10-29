import {
  RATE_EPSILON,
  RATE_TIME_CONSTANT_SECONDS,
  SEEK_FADE_SECONDS,
  MS_PER_SECOND
} from './audio-constants'
import { positionAt, type AudioLoop, type PlaybackAnchor } from './follow-math'

/**
 * One AudioBufferSourceNode playing a deck's buffer, behind its own fade gain.
 * Playback is replaced (not seeked) when it must jump: the old one fades out
 * while the new one fades in, which keeps the splice click-free.
 */
export class Playback {
  private readonly source: AudioBufferSourceNode
  private readonly fade: GainNode
  private anchor: PlaybackAnchor
  private loop: AudioLoop | null = null

  constructor(
    private readonly context: AudioContext,
    buffer: AudioBuffer,
    destination: AudioNode,
    startMs: number,
    rate: number,
    loop: AudioLoop | null
  ) {
    const now = context.currentTime
    this.fade = context.createGain()
    this.fade.gain.setValueAtTime(0, now)
    this.fade.gain.linearRampToValueAtTime(1, now + SEEK_FADE_SECONDS)
    this.fade.connect(destination)

    this.source = context.createBufferSource()
    this.source.buffer = buffer
    this.source.playbackRate.value = rate
    this.source.connect(this.fade)
    this.source.onended = () => this.fade.disconnect()
    this.anchor = { contextTimeSeconds: now, positionMs: startMs, rate }
    this.applyLoop(loop)
    this.source.start(now, startMs / MS_PER_SECOND)
  }

  /** Where the audio actually is, read off the audio clock. */
  positionMs(): number {
    return positionAt(this.anchor, this.context.currentTime, this.loop)
  }

  /** Follows a new rate and loop in place, without restarting the source. */
  update(rate: number, loop: AudioLoop | null): void {
    if (Math.abs(rate - this.anchor.rate) > RATE_EPSILON) {
      const now = this.context.currentTime
      this.anchor = { contextTimeSeconds: now, positionMs: this.positionMs(), rate }
      this.source.playbackRate.setTargetAtTime(rate, now, RATE_TIME_CONSTANT_SECONDS)
    }
    if (!sameLoop(this.loop, loop)) this.applyLoop(loop)
  }

  /** Fades out and stops; the nodes free themselves once the source ends. */
  release(): void {
    const now = this.context.currentTime
    this.fade.gain.cancelScheduledValues(now)
    this.fade.gain.setTargetAtTime(0, now, SEEK_FADE_SECONDS / 3)
    this.source.stop(now + SEEK_FADE_SECONDS)
  }

  private applyLoop(loop: AudioLoop | null): void {
    this.loop = loop
    if (loop === null) {
      this.source.loop = false
      return
    }
    this.source.loopStart = loop.startMs / MS_PER_SECOND
    this.source.loopEnd = loop.endMs / MS_PER_SECOND
    this.source.loop = true
  }
}

function sameLoop(a: AudioLoop | null, b: AudioLoop | null): boolean {
  if (a === null || b === null) return a === b
  return a.startMs === b.startMs && a.endMs === b.endMs
}
