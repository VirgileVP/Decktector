/**
 * A `requestAnimationFrame` loop that only runs while its work can be seen.
 *
 * The deck panels drive their waveforms, beat lights and countdowns from
 * animation loops. Left to themselves those loops keep drawing at 60 fps for a
 * minimised window, a panel scrolled out of view or a deck hidden behind a
 * settings dialog — all of it billed to the WebKit GPU process, which is where
 * Canvas2D is actually rasterised. Gating them on visibility costs one
 * observer per loop and gives all of that back.
 *
 * `target`, when given, pauses the loop while that element is off-screen or
 * `display: none`. Loops resume from the store, so nothing has to be caught up
 * after a pause.
 */
export function startFrameLoop(tick: () => void, target?: Element | null): () => void {
  let raf = 0
  let documentVisible = !document.hidden
  // Assumed visible until the observer reports otherwise, so the first frame
  // is not held back waiting for it.
  let targetVisible = true

  const run = (): void => {
    raf = requestAnimationFrame(run)
    tick()
  }

  const sync = (): void => {
    const shouldRun = documentVisible && targetVisible
    if (shouldRun && raf === 0) {
      raf = requestAnimationFrame(run)
    } else if (!shouldRun && raf !== 0) {
      cancelAnimationFrame(raf)
      raf = 0
    }
  }

  const onVisibilityChange = (): void => {
    documentVisible = !document.hidden
    sync()
  }
  document.addEventListener('visibilitychange', onVisibilityChange)

  let observer: IntersectionObserver | null = null
  if (target && typeof IntersectionObserver !== 'undefined') {
    observer = new IntersectionObserver((entries) => {
      const last = entries[entries.length - 1]
      if (!last) return
      targetVisible = last.isIntersecting
      sync()
    })
    observer.observe(target)
  }

  sync()

  return () => {
    document.removeEventListener('visibilitychange', onVisibilityChange)
    observer?.disconnect()
    if (raf !== 0) cancelAnimationFrame(raf)
    raf = 0
  }
}
