/**
 * Custom chrome for panel windows. Every panel window is created
 * `decorations: false` (see `open-panel-window.ts`) so transparency can be a
 * CSS toggle instead of a create-time-only flag — which means the title
 * bar, the resize border and the close control all have to be reimplemented
 * in HTML, or the window could not be moved, resized or closed at all.
 */

import { useEffect, useState } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'

import {
  applyPanelWindowOptions,
  DEFAULT_PANEL_WINDOW_OPTIONS,
  type PanelWindowOptions
} from './panel-window-options'
import { UI_SCALE_RANGE } from '../state/settings'

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
  const [options, setOptions] = useState<PanelWindowOptions>(DEFAULT_PANEL_WINDOW_OPTIONS)

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent): void => {
      if (event.key === 'Escape') {
        void getCurrentWindow().close()
      }
    }
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [])

  // Lost on close in this task — task 5.2 persists it via the panel windows store.
  useEffect(() => applyPanelWindowOptions(options), [options])

  const toggleAlwaysOnTop = (): void =>
    setOptions((current) => ({ ...current, alwaysOnTop: !current.alwaysOnTop }))

  const toggleTransparent = (): void =>
    setOptions((current) => ({ ...current, transparent: !current.transparent }))

  const adjustUiScale = (delta: number): void =>
    setOptions((current) => ({
      ...current,
      uiScale: Math.min(UI_SCALE_RANGE.max, Math.max(UI_SCALE_RANGE.min, current.uiScale + delta))
    }))

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
        <div className="panel-chrome-controls">
          <button
            type="button"
            className={`panel-chrome-toggle${options.alwaysOnTop ? ' active' : ''}`}
            title="Always on top"
            aria-label="Always on top"
            aria-pressed={options.alwaysOnTop}
            onClick={toggleAlwaysOnTop}
          >
            <svg viewBox="0 0 12 12" width="11" height="11" aria-hidden="true">
              <path d="M6 1v5.2M3 6.2h6L7.6 8.6H4.4L6 6.2ZM6 8.6V11" />
            </svg>
          </button>
          <button
            type="button"
            className={`panel-chrome-toggle${options.transparent ? ' active' : ''}`}
            title="Transparent background"
            aria-label="Transparent background"
            aria-pressed={options.transparent}
            onClick={toggleTransparent}
          >
            <svg viewBox="0 0 12 12" width="11" height="11" aria-hidden="true">
              <path d="M1.5 1.5h4v4h-4zM6.5 6.5h4v4h-4z" />
            </svg>
          </button>
          <button
            type="button"
            className="panel-chrome-zoom"
            title="Zoom out"
            aria-label="Zoom out"
            onClick={() => adjustUiScale(-UI_SCALE_RANGE.step)}
          >
            −
          </button>
          <button
            type="button"
            className="panel-chrome-zoom"
            title="Zoom in"
            aria-label="Zoom in"
            onClick={() => adjustUiScale(UI_SCALE_RANGE.step)}
          >
            +
          </button>
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
