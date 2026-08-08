/**
 * Custom chrome for panel windows. Every panel window is created
 * `decorations: false` (see `open-panel-window.ts`) so transparency can be a
 * CSS toggle instead of a create-time-only flag — which means the title
 * bar, the resize border and the close control all have to be reimplemented
 * in HTML, or the window could not be moved, resized or closed at all.
 */

import { useEffect } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'

/** Shared with `main.css` through `--panel-chrome-height` — not duplicated there. */
const CHROME_HEIGHT_PX = 28

/** Shared with `main.css` through `--panel-resize-edge` — not duplicated there. */
const RESIZE_EDGE_PX = 6

// Not exported by `@tauri-apps/api/window`; mirrors its internal union so the
// literals below still type-check against `startResizeDragging`.
type ResizeDirection =
  | 'North'
  | 'NorthEast'
  | 'East'
  | 'SouthEast'
  | 'South'
  | 'SouthWest'
  | 'West'
  | 'NorthWest'

interface ResizeZone {
  direction: ResizeDirection
  className: string
}

const RESIZE_ZONES: readonly ResizeZone[] = [
  { direction: 'North', className: 'panel-resize-n' },
  { direction: 'NorthEast', className: 'panel-resize-ne' },
  { direction: 'East', className: 'panel-resize-e' },
  { direction: 'SouthEast', className: 'panel-resize-se' },
  { direction: 'South', className: 'panel-resize-s' },
  { direction: 'SouthWest', className: 'panel-resize-sw' },
  { direction: 'West', className: 'panel-resize-w' },
  { direction: 'NorthWest', className: 'panel-resize-nw' }
]

interface PanelChromeProps {
  title: string
  children: React.ReactNode
}

function startResize(direction: ResizeDirection): void {
  void getCurrentWindow().startResizeDragging(direction)
}

export function PanelChrome({ title, children }: PanelChromeProps): React.JSX.Element {
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent): void => {
      if (event.key === 'Escape') {
        void getCurrentWindow().close()
      }
    }
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [])

  return (
    <div
      className="panel-chrome"
      style={{
        ['--panel-chrome-height' as string]: `${CHROME_HEIGHT_PX}px`,
        ['--panel-resize-edge' as string]: `${RESIZE_EDGE_PX}px`
      }}
    >
      <div
        className="panel-chrome-bar"
        data-tauri-drag-region
        onDoubleClick={() => void getCurrentWindow().toggleMaximize()}
      >
        <span className="panel-chrome-title">{title}</span>
        <button
          type="button"
          className="panel-chrome-close"
          title="Close"
          aria-label="Close"
          onClick={() => void getCurrentWindow().close()}
        >
          <svg viewBox="0 0 10 10" width="10" height="10" aria-hidden="true">
            <path d="M1 1l8 8M9 1l-8 8" />
          </svg>
        </button>
      </div>
      <div className="panel-chrome-content">{children}</div>
      {RESIZE_ZONES.map(({ direction, className }) => (
        <div
          key={direction}
          className={`panel-resize-edge ${className}`}
          onMouseDown={() => startResize(direction)}
        />
      ))}
    </div>
  )
}
