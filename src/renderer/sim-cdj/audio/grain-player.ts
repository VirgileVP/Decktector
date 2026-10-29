import { MS_PER_SECOND, SCRATCH_GRAIN_FADE_SECONDS } from './audio-constants'
import type { ScratchGrain } from './scratch-math'

/** Plays one scratch grain through its own faded gain; both nodes free themselves afterwards. */
export function playGrain(
  context: AudioContext,
  buffer: AudioBuffer,
  destination: AudioNode,
  grain: ScratchGrain
): void {
  const startSeconds = grain.startMs / MS_PER_SECOND
  if (startSeconds >= buffer.duration) return

  const now = context.currentTime
  const end = now + grain.durationSeconds
  const envelope = context.createGain()
  envelope.gain.setValueAtTime(0, now)
  envelope.gain.linearRampToValueAtTime(1, now + SCRATCH_GRAIN_FADE_SECONDS)
  envelope.gain.setValueAtTime(
    1,
    Math.max(now + SCRATCH_GRAIN_FADE_SECONDS, end - SCRATCH_GRAIN_FADE_SECONDS)
  )
  envelope.gain.linearRampToValueAtTime(0, end)
  envelope.connect(destination)

  const source = context.createBufferSource()
  source.buffer = buffer
  source.playbackRate.value = grain.rate
  source.connect(envelope)
  source.onended = () => envelope.disconnect()
  source.start(now, startSeconds, grain.durationSeconds * grain.rate)
}
