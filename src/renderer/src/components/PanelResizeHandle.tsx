import { useRef } from 'react'

interface DragStart {
  pointerId: number
  startX: number
  startWidth: number
}

interface PanelResizeHandleProps {
  width: number
  min: number
  max: number
  onResize: (width: number) => void
}

/** Vertical grip between two deck-body rails; drags the rail to its left. */
export function PanelResizeHandle({
  width,
  min,
  max,
  onResize
}: PanelResizeHandleProps): React.JSX.Element {
  const dragRef = useRef<DragStart | null>(null)

  const onPointerDown = (event: React.PointerEvent<HTMLDivElement>): void => {
    if (event.button !== 0) return
    event.currentTarget.setPointerCapture(event.pointerId)
    dragRef.current = { pointerId: event.pointerId, startX: event.clientX, startWidth: width }
  }

  const onPointerMove = (event: React.PointerEvent<HTMLDivElement>): void => {
    const drag = dragRef.current
    if (!drag || event.pointerId !== drag.pointerId) return
    const next = drag.startWidth + (event.clientX - drag.startX)
    onResize(Math.min(max, Math.max(min, next)))
  }

  const endDrag = (event: React.PointerEvent<HTMLDivElement>): void => {
    if (dragRef.current?.pointerId !== event.pointerId) return
    event.currentTarget.releasePointerCapture(event.pointerId)
    dragRef.current = null
  }

  return (
    <div
      className="panel-resize-handle"
      role="separator"
      aria-orientation="vertical"
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={endDrag}
      onPointerCancel={endDrag}
    />
  )
}
