/** Platter speed of a real CDJ at 33 1/3 rpm: one revolution takes this long. */
export const JOG_MS_PER_REVOLUTION = 1800

/** Share of the jog radius (from the centre) that is the touch-sensitive top; beyond it is the edge. */
export const JOG_TOP_RADIUS_RATIO = 0.72

/** Edge drag speed, in revolutions per second, that maps to the full +-1 bend. */
export const NUDGE_FULL_BEND_REVOLUTIONS_PER_SECOND = 0.6

/** Bends closer than this to the last sent one are not worth another message. */
export const NUDGE_MIN_BEND_STEP = 0.02

/** An edge drag that stops moving for this long releases the bend. */
export const NUDGE_IDLE_RELEASE_MS = 90

/** Weight of the newest sample when smoothing the edge's angular velocity. */
export const NUDGE_VELOCITY_SMOOTHING = 0.5

const FULL_TURN = Math.PI * 2
const DEGREES_PER_TURN = 360

export interface Point {
  x: number
  y: number
}

export function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value))
}

/** Angle of `point` around `center` in radians; screen y points down, so clockwise grows. */
export function angleAround(center: Point, point: Point): number {
  return Math.atan2(point.y - center.y, point.x - center.x)
}

/** Signed angle from `previous` to `next`, folded into (-PI, PI] so crossing the atan2 seam is smooth. */
export function angleDelta(previous: number, next: number): number {
  let delta = next - previous
  while (delta > Math.PI) delta -= FULL_TURN
  while (delta <= -Math.PI) delta += FULL_TURN
  return delta
}

/** Track time a platter turn of `deltaRadians` scrubs; clockwise moves forward. */
export function scratchDeltaMs(deltaRadians: number): number {
  return (deltaRadians / FULL_TURN) * JOG_MS_PER_REVOLUTION
}

/** Platter marker angle in degrees for a track position: it turns with the audio. */
export function platterAngleDegrees(positionMs: number): number {
  return ((positionMs / JOG_MS_PER_REVOLUTION) * DEGREES_PER_TURN) % DEGREES_PER_TURN
}

/** Bend in -1..1 for an angular velocity in radians per second. */
export function bendForVelocity(radiansPerSecond: number): number {
  const revolutionsPerSecond = radiansPerSecond / FULL_TURN
  return clamp(revolutionsPerSecond / NUDGE_FULL_BEND_REVOLUTIONS_PER_SECOND, -1, 1)
}

/** Whether a pointer at `point` is on the touch-sensitive top rather than the edge. */
export function isOnJogTop(center: Point, radius: number, point: Point): boolean {
  return Math.hypot(point.x - center.x, point.y - center.y) <= radius * JOG_TOP_RADIUS_RATIO
}
