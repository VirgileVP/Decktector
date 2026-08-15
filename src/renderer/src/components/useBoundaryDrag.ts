import {
  type Dispatch,
  type RefObject,
  type SetStateAction,
  useEffect,
  useLayoutEffect,
  useRef,
  useState
} from 'react'

import { boundaryLimits, type Boundary, type ResolvedSlot } from '../lib/lineup'
import { type AxisMinutes } from '../lib/lineup-axis'
import type { BoundaryPatch } from '../state/lineup'
import {
  FINE_SNAP_MINUTES,
  SNAP_MINUTES,
  ZOOM_STEP,
  minutesToPx,
  panByPx,
  pxToMinutes,
  snapMinutes,
  zoomAt,
  type TimelineViewport
} from '../lib/timeline-scale'

/** How close to a block's edge a grab counts as resizing rather than moving, in pixels. */
const EDGE_GRAB_PX = 6

type Limits = { min: AxisMinutes; max: AxisMinutes }

type DragState =
  | { kind: 'idle' }
  | {
      kind: 'resize'
      id: string
      pointerId: number
      grabOffsetPx: number
      limits: Limits
      original: AxisMinutes
    }
  | {
      kind: 'move'
      /** The block being moved, purely for the `.rail-block-dragging` styling. */
      blockId: string
      pointerId: number
      grabOffsetPx: number
      startId: string
      startLimits: Limits
      startOriginal: AxisMinutes
      endId: string
      endLimits: Limits
      endOriginal: AxisMinutes
    }
  | { kind: 'pan'; pointerId: number; anchorClientX: number; anchorViewport: TimelineViewport }

interface UseBoundaryDragParams {
  viewportRef: RefObject<HTMLDivElement | null>
  viewport: TimelineViewport
  setViewport: Dispatch<SetStateAction<TimelineViewport>>
  boundaries: readonly Boundary[]
  moveBoundaries: (patches: BoundaryPatch[]) => void
  addSlotAt: (minutes: AxisMinutes) => string
  onSlotAdded: (id: string) => void
  fitViewport: () => void
}

interface UseBoundaryDragResult {
  /** Id of the boundary currently being resized, for the `.rail-handle-dragging` styling. */
  draggingId: string | null
  /** Id of the block currently being moved (dragged by its body), for `.rail-block-dragging`. */
  movingId: string | null
  onBlockPointerDown: (event: React.PointerEvent, block: ResolvedSlot) => void
  onTrackPointerDown: (event: React.PointerEvent) => void
  onTrackDoubleClick: (event: React.MouseEvent) => void
  onRulerDoubleClick: () => void
}

function clamp(value: number, limits: Limits): number {
  return Math.min(limits.max, Math.max(limits.min, value))
}

/**
 * The rail's pointer state machine: resizing one edge of a block, moving a
 * whole block, panning the viewport, or idle. Also owns wheel zoom/pan, since
 * both act on the same viewport this machine reads and writes.
 *
 * Grabbing a block's body moves it — both the boundary it starts on and the
 * one it ends on (its successor's start) travel together by the same delta,
 * so the block keeps its length and just slides. Grabbing within
 * `EDGE_GRAB_PX` of either edge instead resizes that one boundary alone,
 * stretching or shrinking the block.
 *
 * Pointer capture is taken on the viewport element, never on a block — a
 * block re-renders and moves under the cursor mid-drag as the store updates,
 * and a capture target that reflows is exactly what breaks.
 */
export function useBoundaryDrag({
  viewportRef,
  viewport,
  setViewport,
  boundaries,
  moveBoundaries,
  addSlotAt,
  onSlotAdded,
  fitViewport
}: UseBoundaryDragParams): UseBoundaryDragResult {
  const dragStateRef = useRef<DragState>({ kind: 'idle' })
  const [draggingId, setDraggingId] = useState<string | null>(null)
  const [movingId, setMovingId] = useState<string | null>(null)

  // Kept current every render so the pointer handlers (mounted once) never read stale props.
  const boundariesRef = useRef(boundaries)
  const viewportRefValue = useRef(viewport)
  useLayoutEffect(() => {
    boundariesRef.current = boundaries
    viewportRefValue.current = viewport
  })

  const endDrag = (): void => {
    const state = dragStateRef.current
    if (state.kind === 'idle') return
    viewportRef.current?.releasePointerCapture(state.pointerId)
    dragStateRef.current = { kind: 'idle' }
    setDraggingId(null)
    setMovingId(null)
  }

  useEffect(() => {
    const el = viewportRef.current
    if (!el) return undefined

    const onPointerMove = (event: PointerEvent): void => {
      const state = dragStateRef.current
      const rect = el.getBoundingClientRect()

      if (state.kind === 'resize') {
        const pointerPx = event.clientX - rect.left
        const desiredPx = pointerPx - state.grabOffsetPx
        const granularity = event.altKey ? FINE_SNAP_MINUTES : SNAP_MINUTES
        const snapped = snapMinutes(pxToMinutes(viewportRefValue.current, desiredPx), granularity)
        moveBoundaries([{ id: state.id, minutes: clamp(snapped, state.limits) }])
      } else if (state.kind === 'move') {
        const pointerPx = event.clientX - rect.left
        const desiredStartPx = pointerPx - state.grabOffsetPx
        const granularity = event.altKey ? FINE_SNAP_MINUTES : SNAP_MINUTES
        const snappedStart = snapMinutes(pxToMinutes(viewportRefValue.current, desiredStartPx), granularity)
        const rawDelta = snappedStart - state.startOriginal

        // Both ends travel by the same amount, so the block's length never changes —
        // the drag simply stops early on whichever side hits its neighbor first.
        const deltaMin = Math.max(
          state.startLimits.min - state.startOriginal,
          state.endLimits.min - state.endOriginal
        )
        const deltaMax = Math.min(
          state.startLimits.max - state.startOriginal,
          state.endLimits.max - state.endOriginal
        )
        const delta = Math.min(deltaMax, Math.max(deltaMin, rawDelta))

        moveBoundaries([
          { id: state.startId, minutes: state.startOriginal + delta },
          { id: state.endId, minutes: state.endOriginal + delta }
        ])
      } else if (state.kind === 'pan') {
        setViewport(panByPx(state.anchorViewport, state.anchorClientX - event.clientX))
      }
    }

    const onPointerUp = (): void => endDrag()

    const restoreOriginal = (): void => {
      const state = dragStateRef.current
      if (state.kind === 'resize') {
        moveBoundaries([{ id: state.id, minutes: state.original }])
      } else if (state.kind === 'move') {
        moveBoundaries([
          { id: state.startId, minutes: state.startOriginal },
          { id: state.endId, minutes: state.endOriginal }
        ])
      }
    }

    const onPointerCancel = (): void => {
      restoreOriginal()
      endDrag()
    }

    const onKeyDown = (event: KeyboardEvent): void => {
      if (event.key !== 'Escape') return
      if (dragStateRef.current.kind === 'idle' || dragStateRef.current.kind === 'pan') return
      restoreOriginal()
      endDrag()
    }

    const onWheel = (event: WheelEvent): void => {
      event.preventDefault()
      const rect = el.getBoundingClientRect()
      if (event.shiftKey) {
        setViewport((current) => panByPx(current, event.deltaY !== 0 ? event.deltaY : event.deltaX))
        return
      }
      const factor = event.deltaY > 0 ? 1 / ZOOM_STEP : ZOOM_STEP
      setViewport((current) => zoomAt(current, event.clientX - rect.left, factor))
    }

    el.addEventListener('pointermove', onPointerMove)
    el.addEventListener('pointerup', onPointerUp)
    el.addEventListener('pointercancel', onPointerCancel)
    el.addEventListener('wheel', onWheel, { passive: false })
    window.addEventListener('keydown', onKeyDown)

    return () => {
      el.removeEventListener('pointermove', onPointerMove)
      el.removeEventListener('pointerup', onPointerUp)
      el.removeEventListener('pointercancel', onPointerCancel)
      el.removeEventListener('wheel', onWheel)
      window.removeEventListener('keydown', onKeyDown)
    }
    // viewportRef's element identity is stable for the rail's lifetime.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  const startPan = (event: React.PointerEvent): void => {
    const el = viewportRef.current
    if (!el) return
    el.setPointerCapture(event.pointerId)
    dragStateRef.current = {
      kind: 'pan',
      pointerId: event.pointerId,
      anchorClientX: event.clientX,
      anchorViewport: viewportRefValue.current
    }
  }

  const startResize = (event: React.PointerEvent, boundary: Boundary): void => {
    const el = viewportRef.current
    if (!el) return
    const rect = el.getBoundingClientRect()
    const pointerPx = event.clientX - rect.left
    const boundaryPx = minutesToPx(viewportRefValue.current, boundary.minutes)

    el.setPointerCapture(event.pointerId)
    dragStateRef.current = {
      kind: 'resize',
      id: boundary.id,
      pointerId: event.pointerId,
      grabOffsetPx: pointerPx - boundaryPx,
      limits: boundaryLimits(boundariesRef.current, boundary.id),
      original: boundary.minutes
    }
    setDraggingId(boundary.id)
  }

  const startMove = (event: React.PointerEvent, blockId: string, start: Boundary, end: Boundary): void => {
    const el = viewportRef.current
    if (!el) return
    const rect = el.getBoundingClientRect()
    const pointerPx = event.clientX - rect.left
    const startPx = minutesToPx(viewportRefValue.current, start.minutes)

    el.setPointerCapture(event.pointerId)
    dragStateRef.current = {
      kind: 'move',
      blockId,
      pointerId: event.pointerId,
      grabOffsetPx: pointerPx - startPx,
      startId: start.id,
      startLimits: boundaryLimits(boundariesRef.current, start.id),
      startOriginal: start.minutes,
      endId: end.id,
      endLimits: boundaryLimits(boundariesRef.current, end.id),
      endOriginal: end.minutes
    }
    setMovingId(blockId)
  }

  const onBlockPointerDown = (event: React.PointerEvent, block: ResolvedSlot): void => {
    if (event.button === 1) {
      event.preventDefault()
      startPan(event)
      return
    }
    if (event.button !== 0) return

    const index = boundaries.findIndex((boundary) => boundary.id === block.id)
    if (index === -1) return
    // A block with no next boundary (END SHOW) has nothing to resize into or move against.
    const hasNext = index < boundaries.length - 1
    const rect = event.currentTarget.getBoundingClientRect()
    const nearLeftEdge = event.clientX - rect.left <= EDGE_GRAB_PX
    const nearRightEdge = hasNext && rect.right - event.clientX <= EDGE_GRAB_PX

    event.stopPropagation()

    if (nearRightEdge) {
      startResize(event, boundaries[index + 1])
    } else if (nearLeftEdge || !hasNext) {
      startResize(event, boundaries[index])
    } else {
      startMove(event, block.id, boundaries[index], boundaries[index + 1])
    }
  }

  const onTrackPointerDown = (event: React.PointerEvent): void => {
    if (event.button === 1) {
      event.preventDefault()
      startPan(event)
      return
    }
    if (event.button !== 0) return
    startPan(event)
  }

  const onTrackDoubleClick = (event: React.MouseEvent): void => {
    const el = viewportRef.current
    if (!el) return
    const rect = el.getBoundingClientRect()
    const minutes = snapMinutes(pxToMinutes(viewport, event.clientX - rect.left), SNAP_MINUTES)
    const id = addSlotAt(minutes)
    onSlotAdded(id)
  }

  const onRulerDoubleClick = (): void => fitViewport()

  return {
    draggingId,
    movingId,
    onBlockPointerDown,
    onTrackPointerDown,
    onTrackDoubleClick,
    onRulerDoubleClick
  }
}
