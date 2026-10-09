import { useEffect, useLayoutEffect, useReducer, useState, type RefObject } from 'react'

/**
 * Box aspect ratio (width / height) from which the content is laid out on one
 * row instead of stacked. Sits between the two layouts' own aspect ratios —
 * the row arrangement is roughly 10:1, the stacked one roughly 1:1 — so each
 * wins on the box shapes it actually fills best.
 */
const ROW_LAYOUT_MIN_ASPECT = 3.5

/**
 * Box aspect ratio (width / height) below which the box counts as portrait:
 * taller than wide, so anything that can go under its neighbour instead of
 * beside it should — width is what limits the scale there.
 */
const PORTRAIT_MAX_ASPECT = 1

/** Share of the box the fitted content fills, leaving a margin of air around it. */
const FILL_RATIO = 0.94

/** Keeps a degenerate measurement from producing absurdly small or large text. */
const SCALE_RANGE = { min: 0.4, max: 12 } as const

/**
 * The box's shape, for the content to arrange itself by: `wide` is wide
 * enough for a single row, `landscape` is wider than tall without being
 * that wide, `portrait` is taller than wide.
 */
export type BoxShape = 'wide' | 'landscape' | 'portrait'

export interface FitToBox {
  /** Multiplier to apply to the content's natural size so it fills the box. */
  scale: number
  shape: BoxShape
}

/** What a caller that opted out reads, so the content renders at its natural size. */
const NO_FIT: FitToBox = { scale: 1, shape: 'wide' }

/** Classifies a box of `width` by `height` (both positive). */
export function boxShape(width: number, height: number): BoxShape {
  const aspect = width / height
  if (aspect >= ROW_LAYOUT_MIN_ASPECT) return 'wide'
  if (aspect < PORTRAIT_MAX_ASPECT) return 'portrait'
  return 'landscape'
}

function clampScale(scale: number): number {
  return Math.min(SCALE_RANGE.max, Math.max(SCALE_RANGE.min, scale))
}

/**
 * Scales the element in `contentRef` to fill the one in `boxRef`, and reports
 * the box's shape for the content to pick its arrangement from.
 *
 * This replaces sizing text off viewport units: a `clamp()` of `vw`/`vh` has
 * to guess how much text there is, so it either clips a short window or leaves
 * a tall one nearly empty. Measuring what the content actually needs holds for
 * any window shape and any number of sets, and needs no per-breakpoint tuning.
 *
 * The caller applies `scale` itself (a CSS transform), which is what keeps this
 * loop-free: `offsetWidth`/`offsetHeight` report the pre-transform layout size,
 * so the applied scale never feeds back into the next measurement.
 */
export function useFitToBox(
  boxRef: RefObject<HTMLElement | null>,
  contentRef: RefObject<HTMLElement | null>,
  enabled: boolean
): FitToBox {
  const [fit, setFit] = useState<FitToBox>(NO_FIT)
  const [resizeTick, signalResize] = useReducer((tick: number) => tick + 1, 0)

  // `fit.shape` belongs in the dependencies below because flipping the layout is
  // itself what changes the content's size: a scale measured in the same pass
  // that decides to flip describes the arrangement being replaced, so the
  // measurement has to run once more against the new one. That settles in a
  // single extra pass — the content's size does not depend on the scale, so
  // nothing here can chain into an endless chain of updates.
  useLayoutEffect(() => {
    if (!enabled) return
    const box = boxRef.current
    const content = contentRef.current
    if (box === null || content === null) return

    const boxRect = box.getBoundingClientRect()
    const contentWidth = content.offsetWidth
    const contentHeight = content.offsetHeight
    // A box or content of zero extent carries no ratio to fit to — this happens
    // between mount and first layout, and a later pass sees real numbers.
    if (boxRect.width <= 0 || boxRect.height <= 0) return
    if (contentWidth <= 0 || contentHeight <= 0) return

    const next: FitToBox = {
      scale: clampScale(
        FILL_RATIO * Math.min(boxRect.width / contentWidth, boxRect.height / contentHeight)
      ),
      shape: boxShape(boxRect.width, boxRect.height)
    }
    setFit((current) =>
      current.scale === next.scale && current.shape === next.shape ? current : next
    )
  }, [boxRef, contentRef, enabled, resizeTick, fit.shape])

  useEffect(() => {
    if (!enabled) return undefined
    const box = boxRef.current
    const content = contentRef.current
    if (box === null || content === null) return undefined

    // Both ends matter: the box changes when the window is resized, the content
    // when the lineup is edited or a web font finally loads. The observer only
    // has to schedule a render — the layout effect above does the measuring.
    const observer = new ResizeObserver(signalResize)
    observer.observe(box)
    observer.observe(content)
    // A panel window's box tracks the viewport, so this covers the main case
    // directly — `ResizeObserver` only delivers as part of a rendering step,
    // which a host that is not painting (an occluded or hidden window) can skip.
    window.addEventListener('resize', signalResize)
    return () => {
      observer.disconnect()
      window.removeEventListener('resize', signalResize)
    }
  }, [boxRef, contentRef, enabled])

  return enabled ? fit : NO_FIT
}
