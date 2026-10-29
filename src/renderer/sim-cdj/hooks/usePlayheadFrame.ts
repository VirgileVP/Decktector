import { useEffect, useRef } from 'react'

import type { DeckId } from '@renderer/model'
import { startFrameLoop } from '@renderer/lib/frame-loop'
import { estimatePlayheadMs } from '@renderer/lib/playhead'
import { useDecktectorStore } from '@renderer/state/store'

/**
 * Calls `onFrame` every animation frame with the deck's interpolated playhead
 * (null while no track is loaded). Imperative on purpose: readouts write to
 * the DOM directly instead of re-rendering React 60 times a second.
 */
export function usePlayheadFrame(
  deckId: DeckId,
  onFrame: (playheadMs: number | null) => void
): void {
  const onFrameRef = useRef(onFrame)
  useEffect(() => {
    onFrameRef.current = onFrame
  })

  useEffect(
    () =>
      startFrameLoop(() => {
        const deck = useDecktectorStore.getState().decks[deckId]
        onFrameRef.current(estimatePlayheadMs(deck, Date.now()))
      }),
    [deckId]
  )
}
