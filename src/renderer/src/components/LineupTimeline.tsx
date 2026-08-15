import { useEffect, useRef, useState } from 'react'

import {
  DOORS_OPEN_ID,
  END_SHOW_ID,
  boundaryLimits,
  lineupBlocks,
  lineupBoundaries,
  lineupSpan,
  projectClockOnAxis,
  type ResolvedSlot
} from '../lib/lineup'
import { AXIS_MINUTES, MINUTES_PER_DAY, formatAxisLabel } from '../lib/lineup-axis'
import {
  clampViewport,
  fitRange,
  minutesToPx,
  tickStep,
  visibleSpan,
  type TimelineViewport
} from '../lib/timeline-scale'
import { useLineup } from '../state/lineup'
import { useClockMinutes } from '../state/wall-clock'
import { useBoundaryDrag } from './useBoundaryDrag'

/** Padding kept around the used span when the rail first frames itself. */
const FIT_PAD_MINUTES = 30

interface LineupTimelineProps {
  /** Called after a set is added by double-clicking the track, so the caller can focus its row. */
  onSlotAdded?: (id: string) => void
}

/**
 * The rail: a zoomable, draggable timeline over the night, sitting above the
 * per-set rows in the editor. Plain positioned divs, not canvas — a handful
 * of blocks and a few dozen ticks are cheap in the DOM, and it gets text
 * ellipsis, hover states and focusable handles for free.
 */
export function LineupTimeline({ onSlotAdded }: LineupTimelineProps): React.JSX.Element {
  const slots = useLineup((s) => s.slots)
  const endOffset = useLineup((s) => s.endOffset)
  const doorsOffset = useLineup((s) => s.doorsOffset)
  const moveBoundaries = useLineup((s) => s.moveBoundaries)
  const addSlotAt = useLineup((s) => s.addSlotAt)
  const nowClockMinutes = useClockMinutes()

  const viewportRef = useRef<HTMLDivElement>(null)
  const [widthPx, setWidthPx] = useState(0)
  const [viewport, setViewport] = useState<TimelineViewport>({
    originMinutes: 0,
    pxPerMinute: 1,
    widthPx: 0
  })
  const hasFitRef = useRef(false)

  const boundaries = lineupBoundaries(slots, endOffset, doorsOffset)
  const blocks = lineupBlocks(slots, endOffset, doorsOffset, { includeUnnamed: true })
  const span = lineupSpan(slots, endOffset, doorsOffset)
  const nowAxis = projectClockOnAxis(nowClockMinutes, span)

  const fitViewport = (): void => {
    if (widthPx <= 0) return
    const target = span ?? { startOffset: 0, endOffset: AXIS_MINUTES }
    setViewport(fitRange(widthPx, target.startOffset, target.endOffset, FIT_PAD_MINUTES))
  }

  useEffect(() => {
    const el = viewportRef.current
    if (!el) return undefined
    // The ResizeObserver's first callback isn't guaranteed to land before the next paint in
    // every host, so an immediate synchronous read seeds widthPx without waiting on it.
    setWidthPx(el.getBoundingClientRect().width)
    const observer = new ResizeObserver((entries) => {
      const width = entries[0]?.contentRect.width
      if (width !== undefined) setWidthPx(width)
    })
    observer.observe(el)
    return () => observer.disconnect()
  }, [])

  useEffect(() => {
    if (widthPx <= 0) return
    if (!hasFitRef.current) {
      hasFitRef.current = true
      fitViewport()
      return
    }
    setViewport((current) =>
      current.widthPx === widthPx ? current : clampViewport({ ...current, widthPx })
    )
    // fitViewport reads widthPx/span from the render closure by design — it only needs to
    // re-run on the width itself, never on a span change (that would fight the user's zoom).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [widthPx])

  const {
    draggingId,
    movingId,
    onBlockPointerDown,
    onTrackPointerDown,
    onTrackDoubleClick,
    onRulerDoubleClick
  } = useBoundaryDrag({
      viewportRef,
      viewport,
      setViewport,
      boundaries,
      moveBoundaries,
      addSlotAt,
      onSlotAdded: (id) => onSlotAdded?.(id),
      fitViewport
    })

  if (boundaries.length === 0) {
    return (
      <div className="rail rail-empty">
        <p className="rail-hint">Add a set below to see it on the timeline.</p>
      </div>
    )
  }

  const step = tickStep(viewport.pxPerMinute)
  const ticks = buildTicks(viewport, step)
  const nowVisible = viewport.widthPx > 0

  return (
    <div className="rail">
      <div
        className="rail-ruler"
        onDoubleClick={onRulerDoubleClick}
        title="Double-click to fit the used span"
      >
        {ticks.map((tick) => (
          <span
            key={tick.minutes}
            className={`rail-tick${tick.isDay ? ' rail-tick-day' : ''}`}
            style={{ left: `${minutesToPx(viewport, tick.minutes)}px` }}
          >
            <span className="rail-tick-label">{formatAxisLabel(tick.minutes)}</span>
          </span>
        ))}
      </div>

      <div
        ref={viewportRef}
        className="rail-viewport"
        onPointerDown={onTrackPointerDown}
        onDoubleClick={onTrackDoubleClick}
      >
        {blocks.map((block) => (
          <RailBlock
            key={block.id}
            block={block}
            viewport={viewport}
            isMoving={movingId === block.id}
            onPointerDown={onBlockPointerDown}
          />
        ))}

        {boundaries.map((boundary) => {
          const limits = boundaryLimits(boundaries, boundary.id)
          return (
            <div
              key={boundary.id}
              role="slider"
              tabIndex={0}
              aria-label={`${boundary.artist} time`}
              aria-valuemin={limits.min}
              aria-valuemax={limits.max}
              aria-valuenow={boundary.minutes}
              aria-valuetext={formatAxisLabel(boundary.minutes)}
              className={`rail-handle${draggingId === boundary.id ? ' rail-handle-dragging' : ''}`}
              style={{ left: `${minutesToPx(viewport, boundary.minutes)}px` }}
              onKeyDown={(event) => {
                const arrowStep = event.shiftKey ? 60 : 5
                let next: number | null = null
                if (event.key === 'ArrowLeft' || event.key === 'ArrowDown') next = boundary.minutes - arrowStep
                else if (event.key === 'ArrowRight' || event.key === 'ArrowUp')
                  next = boundary.minutes + arrowStep
                else if (event.key === 'Home') next = limits.min
                else if (event.key === 'End') next = limits.max
                if (next === null) return
                event.preventDefault()
                const clamped = Math.min(limits.max, Math.max(limits.min, next))
                moveBoundaries([{ id: boundary.id, minutes: clamped }])
              }}
            />
          )
        })}

        {nowVisible && (
          <div className="rail-now" style={{ left: `${minutesToPx(viewport, nowAxis)}px` }} />
        )}
      </div>
    </div>
  )
}

interface RailBlockProps {
  block: ResolvedSlot
  viewport: TimelineViewport
  isMoving: boolean
  onPointerDown: (event: React.PointerEvent, block: ResolvedSlot) => void
}

function RailBlock({ block, viewport, isMoving, onPointerDown }: RailBlockProps): React.JSX.Element {
  const left = minutesToPx(viewport, block.startOffset)

  if (block.id === END_SHOW_ID) {
    return (
      <div
        className="rail-marker-end"
        style={{ left: `${left}px` }}
        title={block.artist}
        onPointerDown={(event) => onPointerDown(event, block)}
      />
    )
  }

  const width = block.endOffset === null ? 0 : minutesToPx(viewport, block.endOffset) - left
  const isUnnamed = block.artist.trim().length === 0
  const isOpen = block.endOffset === null
  const classes = [
    'rail-block',
    block.id === DOORS_OPEN_ID ? 'rail-block-doors' : '',
    isUnnamed ? 'rail-block-unnamed' : '',
    isOpen ? 'rail-block-open' : '',
    isMoving ? 'rail-block-dragging' : ''
  ]
    .filter(Boolean)
    .join(' ')

  return (
    <div
      className={classes}
      style={{ left: `${left}px`, width: `${Math.max(width, 0)}px` }}
      title={block.artist || undefined}
      onPointerDown={(event) => onPointerDown(event, block)}
    >
      <span className="rail-block-label">{block.artist}</span>
    </div>
  )
}

function buildTicks(viewport: TimelineViewport, step: number): { minutes: number; isDay: boolean }[] {
  const start = Math.max(0, Math.ceil(viewport.originMinutes / step) * step)
  const end = Math.min(AXIS_MINUTES, viewport.originMinutes + visibleSpan(viewport))
  const ticks: { minutes: number; isDay: boolean }[] = []
  for (let minutes = start; minutes <= end; minutes += step) {
    ticks.push({ minutes, isDay: minutes % MINUTES_PER_DAY === 0 })
  }
  return ticks
}
