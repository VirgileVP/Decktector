import { useCallback, useEffect, useRef } from 'react'
import type { PointerEvent } from 'react'

import type { DeckId } from '@renderer/model'

import {
  NUDGE_IDLE_RELEASE_MS,
  NUDGE_MIN_BEND_STEP,
  NUDGE_VELOCITY_SMOOTHING,
  angleAround,
  angleDelta,
  bendForVelocity,
  isOnJogTop,
  scratchDeltaMs
} from '../lib/jog-math'
import { sendCommand } from '../net/sim-connection'

const MS_PER_SECOND = 1000

type JogMode = 'scratch' | 'nudge'

interface Gesture {
  pointerId: number
  mode: JogMode
  lastAngle: number
  lastTimeMs: number
  /** Smoothed edge angular velocity in radians per second. */
  velocity: number
  lastBend: number
}

export interface JogHandlers {
  onPointerDown: (event: PointerEvent<HTMLElement>) => void
  onPointerMove: (event: PointerEvent<HTMLElement>) => void
  onPointerUp: (event: PointerEvent<HTMLElement>) => void
  onPointerCancel: (event: PointerEvent<HTMLElement>) => void
  onLostPointerCapture: (event: PointerEvent<HTMLElement>) => void
}

/**
 * Turns pointer movement around a jog wheel into simulator commands.
 *
 * On the top of the platter the pointer is a finger on the platter: touch,
 * then angular movement scrubs the track. On the edge it is a hand on the
 * rim: the angular speed becomes a bend that speeds up or slows the track,
 * released when the pointer stops or lets go.
 */
export function useJogGestures(deckId: DeckId, enabled: boolean): JogHandlers {
  const gestureRef = useRef<Gesture | null>(null)
  const idleTimerRef = useRef<number | null>(null)

  const clearIdleTimer = useCallback((): void => {
    if (idleTimerRef.current !== null) window.clearTimeout(idleTimerRef.current)
    idleTimerRef.current = null
  }, [])

  const sendBend = useCallback(
    (gesture: Gesture, bend: number): void => {
      gesture.lastBend = bend
      sendCommand({ type: 'jogNudge', deck: deckId, bend })
    },
    [deckId]
  )

  const release = useCallback((): void => {
    const gesture = gestureRef.current
    if (!gesture) return
    gestureRef.current = null
    clearIdleTimer()
    if (gesture.mode === 'scratch') {
      sendCommand({ type: 'jogTouch', deck: deckId, touched: false })
    } else if (gesture.lastBend !== 0) {
      sendCommand({ type: 'jogNudge', deck: deckId, bend: 0 })
    }
  }, [clearIdleTimer, deckId])

  // A deck that loses its track mid-gesture, or a page that goes away, must not leave the jog held.
  useEffect(() => {
    if (!enabled) release()
    return release
  }, [enabled, release])

  const onPointerDown = (event: PointerEvent<HTMLElement>): void => {
    if (!enabled || gestureRef.current) return
    const rect = event.currentTarget.getBoundingClientRect()
    const center = { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 }
    const point = { x: event.clientX, y: event.clientY }
    const mode: JogMode = isOnJogTop(center, rect.width / 2, point) ? 'scratch' : 'nudge'

    event.currentTarget.setPointerCapture(event.pointerId)
    gestureRef.current = {
      pointerId: event.pointerId,
      mode,
      lastAngle: angleAround(center, point),
      lastTimeMs: event.timeStamp,
      velocity: 0,
      lastBend: 0
    }
    if (mode === 'scratch') sendCommand({ type: 'jogTouch', deck: deckId, touched: true })
  }

  const onPointerMove = (event: PointerEvent<HTMLElement>): void => {
    const gesture = gestureRef.current
    if (!gesture || gesture.pointerId !== event.pointerId) return
    const rect = event.currentTarget.getBoundingClientRect()
    const center = { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 }
    const angle = angleAround(center, { x: event.clientX, y: event.clientY })
    const delta = angleDelta(gesture.lastAngle, angle)
    const elapsedSeconds = (event.timeStamp - gesture.lastTimeMs) / MS_PER_SECOND
    gesture.lastAngle = angle
    gesture.lastTimeMs = event.timeStamp
    if (delta === 0) return

    if (gesture.mode === 'scratch') {
      sendCommand({ type: 'jogScratch', deck: deckId, deltaMs: scratchDeltaMs(delta) })
      return
    }

    if (elapsedSeconds <= 0) return
    gesture.velocity =
      NUDGE_VELOCITY_SMOOTHING * (delta / elapsedSeconds) +
      (1 - NUDGE_VELOCITY_SMOOTHING) * gesture.velocity
    const bend = bendForVelocity(gesture.velocity)
    if (Math.abs(bend - gesture.lastBend) >= NUDGE_MIN_BEND_STEP) sendBend(gesture, bend)

    clearIdleTimer()
    idleTimerRef.current = window.setTimeout(() => {
      gesture.velocity = 0
      if (gesture.lastBend !== 0) sendBend(gesture, 0)
    }, NUDGE_IDLE_RELEASE_MS)
  }

  return {
    onPointerDown,
    onPointerMove,
    onPointerUp: release,
    onPointerCancel: release,
    onLostPointerCapture: release
  }
}
