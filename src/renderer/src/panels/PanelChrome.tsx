/**
 * Custom chrome for panel windows. Every panel window is created
 * `decorations: false` (see `open-panel-window.ts`) so transparency can be a
 * CSS toggle instead of a create-time-only flag — which means the title
 * bar, the resize border and the close control all have to be reimplemented
 * in HTML, or the window could not be moved, resized or closed at all.
 */

import { useEffect, useRef, useState } from 'react'
import { isTauri } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'

import {
  applyPanelWindowOptions,
  DEFAULT_PANEL_WINDOW_OPTIONS,
  type PanelWindowOptions
} from './panel-window-options'
import type { PanelDisplay } from './panel-display'
import { reportPanelWindow } from './panel-window-report'
import type { PanelParams } from './params'
import { readWindowGeometry } from './read-window-geometry'
import { useAppShortcuts } from '../components/useAppShortcuts'
import { toggleFullscreen } from '../state/fullscreen'
import { UI_SCALE_RANGE } from '../state/settings'
import { currentPanelWindowRecord } from '../state/panel-windows'

/** Shared with `main.css` through `--panel-chrome-height` — not duplicated there. */
const CHROME_HEIGHT_PX = 28

/** Shared with `main.css` through `--panel-resize-edge` — not duplicated there. */
const RESIZE_EDGE_PX = 6

/** Coalesces a drag or a resize into one report instead of a report per pixel. */
const GEOMETRY_DEBOUNCE_MS = 300

// Not exported by `@tauri-apps/api/window`; mirrors its internal union so the
// literals below still type-check against `startResizeDragging`.
type ResizeDirection =
  'North' | 'NorthEast' | 'East' | 'SouthEast' | 'South' | 'SouthWest' | 'West' | 'NorthWest'

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
  params: PanelParams
  /** What the window shows — owned by `PanelWindow`, reported from here with the rest of the window's state. */
  display: PanelDisplay
  /** A panel-specific control rendered alongside the built-in ones (e.g. the panel's display options). */
  toolbarExtra?: React.ReactNode
  children: React.ReactNode
}

/**
 * Desktop OS window, or a browser popup opened from the remote view. A popup
 * keeps the browser's own frame, so the resize edges, always-on-top and
 * transparency — all window-manager features — only exist in the former.
 */
const IS_TAURI = isTauri()

function startResize(direction: ResizeDirection): void {
  void getCurrentWindow().startResizeDragging(direction)
}

function closeWindow(): void {
  if (IS_TAURI) {
    void getCurrentWindow().close()
  } else {
    window.close()
  }
}

export function PanelChrome({
  title,
  params,
  display,
  toolbarExtra,
  children
}: PanelChromeProps): React.JSX.Element {
  // Seeded from this window's own record — the persisted state a fresh
  // pop-out has no record for yet, and a restored window (task 5.3) does,
  // since the main window wrote it to the same `localStorage` before
  // recreating this window.
  const [options, setOptions] = useState<PanelWindowOptions>(
    () => currentPanelWindowRecord()?.options ?? DEFAULT_PANEL_WINDOW_OPTIONS
  )

  // The chrome bar only reveals on `:hover` (see `main.css`) so a transparent,
  // keyed window stays visually clean while untouched — but hover alone
  // leaves no way to find the controls without a mouse already on the 28px
  // bar. Window focus covers touch (a tap focuses the window) and keyboard,
  // and only fires on an actual interaction, so a keyed window sitting idle
  // during a live set still composites with zero incidental chrome.
  const [windowFocused, setWindowFocused] = useState(() => IS_TAURI || document.hasFocus())

  useEffect(() => {
    if (!IS_TAURI) {
      const handleFocus = (): void => setWindowFocused(true)
      const handleBlur = (): void => setWindowFocused(false)
      window.addEventListener('focus', handleFocus)
      window.addEventListener('blur', handleBlur)
      return () => {
        window.removeEventListener('focus', handleFocus)
        window.removeEventListener('blur', handleBlur)
      }
    }
    const currentWindow = getCurrentWindow()
    let unlistenFocus: (() => void) | null = null
    void currentWindow.isFocused().then(setWindowFocused)
    void currentWindow
      .onFocusChanged(({ payload }) => setWindowFocused(payload))
      .then((stop) => {
        unlistenFocus = stop
      })
    return () => unlistenFocus?.()
  }, [])

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent): void => {
      if (event.key === 'Escape') closeWindow()
    }
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [])

  // Tauri titles the window at creation; a browser popup takes the tab title.
  useEffect(() => {
    if (!IS_TAURI) document.title = title
  }, [title])

  // Settings live in the main window; a panel window only goes fullscreen.
  useAppShortcuts({ toggleFullscreen })

  useEffect(() => applyPanelWindowOptions(options), [options])

  // `reportPanelWindow` needs the latest params/options/display, but the
  // move/resize listeners below must only be attached once — a ref sidesteps
  // re-subscribing them on every option change just to read fresh values.
  const latestReportInputs = useRef({ params, options, display })
  useEffect(() => {
    latestReportInputs.current = { params, options, display }
  }, [params, options, display])

  const reportCurrentState = (): void => {
    void readWindowGeometry().then((geometry) => {
      const {
        params: currentParams,
        options: currentOptions,
        display: currentDisplay
      } = latestReportInputs.current
      reportPanelWindow({
        type: 'update',
        label: getCurrentWindow().label,
        params: currentParams,
        options: currentOptions,
        display: currentDisplay,
        geometry
      })
    })
  }

  // Reports on mount and whenever the panel's own state changes.
  useEffect(() => {
    if (!IS_TAURI) return
    reportCurrentState()
  }, [params, options, display])

  // Reports on move/resize (debounced) and once more, synchronously, when the
  // OS is actually about to close the window — a React unmount is not a
  // reliable place to do this, since the webview can be torn down before its
  // cleanup functions run.
  useEffect(() => {
    if (!IS_TAURI) return

    const currentWindow = getCurrentWindow()
    let debounceTimer: ReturnType<typeof setTimeout> | undefined

    const scheduleReport = (): void => {
      if (debounceTimer !== undefined) clearTimeout(debounceTimer)
      debounceTimer = setTimeout(reportCurrentState, GEOMETRY_DEBOUNCE_MS)
    }

    let unlistenMoved: (() => void) | null = null
    let unlistenResized: (() => void) | null = null
    let unlistenCloseRequested: (() => void) | null = null

    void currentWindow.onMoved(scheduleReport).then((stop) => {
      unlistenMoved = stop
    })
    void currentWindow.onResized(scheduleReport).then((stop) => {
      unlistenResized = stop
    })
    void currentWindow
      .onCloseRequested(() => {
        reportPanelWindow({ type: 'closed', label: currentWindow.label })
      })
      .then((stop) => {
        unlistenCloseRequested = stop
      })

    return () => {
      if (debounceTimer !== undefined) clearTimeout(debounceTimer)
      unlistenMoved?.()
      unlistenResized?.()
      unlistenCloseRequested?.()
    }
  }, [])

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
      data-window-focused={windowFocused}
      style={{
        ['--panel-chrome-height' as string]: `${CHROME_HEIGHT_PX}px`,
        ['--panel-resize-edge' as string]: `${RESIZE_EDGE_PX}px`
      }}
    >
      <div className="panel-chrome-bar">
        {/* Buttons must be siblings of the drag region, not descendants —
            Tauri v2 does not reliably deliver click events to interactive
            elements nested inside `data-tauri-drag-region` (upstream bug,
            tauri-apps/tauri#9901). */}
        <div
          className="panel-chrome-drag"
          data-tauri-drag-region
          onDoubleClick={IS_TAURI ? () => void getCurrentWindow().toggleMaximize() : undefined}
        >
          <span className="panel-chrome-title">{title}</span>
        </div>
        <div className="panel-chrome-controls">
          {IS_TAURI && (
            <>
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
            </>
          )}
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
          {toolbarExtra}
          <button
            type="button"
            className="panel-chrome-close"
            title="Close"
            aria-label="Close"
            onClick={closeWindow}
          >
            <svg viewBox="0 0 10 10" width="10" height="10" aria-hidden="true">
              <path d="M1 1l8 8M9 1l-8 8" />
            </svg>
          </button>
        </div>
      </div>
      <div className="panel-chrome-content">{children}</div>
      {IS_TAURI &&
        RESIZE_ZONES.map(({ direction, className }) => (
          <div
            key={direction}
            className={`panel-resize-edge ${className}`}
            onMouseDown={() => startResize(direction)}
          />
        ))}
    </div>
  )
}
