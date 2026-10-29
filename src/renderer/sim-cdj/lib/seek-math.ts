import { clamp } from './jog-math'

/** Track position under a pointer at `clientX` over a strip spanning `strip` and a track `durationMs` long. */
export function seekPositionMs(
  clientX: number,
  strip: Pick<DOMRect, 'left' | 'width'>,
  durationMs: number
): number {
  if (strip.width <= 0) return 0
  return clamp((clientX - strip.left) / strip.width, 0, 1) * durationMs
}

/** Fraction (0..1) of the track at `positionMs`, for placing overlays on the preview strip. */
export function trackFraction(positionMs: number, durationMs: number): number {
  if (durationMs <= 0) return 0
  return clamp(positionMs / durationMs, 0, 1)
}
