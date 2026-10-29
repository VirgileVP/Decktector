import { describe, expect, it, vi } from 'vitest'

import type { BufferLoader } from './buffer-loader'
import { createDeckAudioSync } from './deck-audio-sync'

const BUFFER_A = { duration: 1 } as AudioBuffer
const BUFFER_B = { duration: 2 } as AudioBuffer

function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (reason: unknown) => void
  const promise = new Promise<T>((res, rej) => {
    resolve = res
    reject = rej
  })
  return { promise, resolve, reject }
}

function setup(loads: Record<string, Promise<AudioBuffer>>) {
  const sink = { setDeckBuffer: vi.fn() }
  const reportStatus = vi.fn()
  const loader: BufferLoader = { load: (url) => loads[url] }
  return { sink, reportStatus, sync: createDeckAudioSync(sink, loader, reportStatus).sync }
}

describe('createDeckAudioSync', () => {
  it('reports a missing file as none', () => {
    const { sync, reportStatus, sink } = setup({})
    sync(1, null)
    expect(reportStatus).toHaveBeenCalledWith(1, 'none')
    expect(sink.setDeckBuffer).toHaveBeenCalledWith(1, null)
  })

  it('goes from loading to ready and hands over the buffer', async () => {
    const { sync, reportStatus, sink } = setup({ '/a': Promise.resolve(BUFFER_A) })
    sync(1, '/a')
    expect(reportStatus).toHaveBeenLastCalledWith(1, 'loading')
    await Promise.resolve()
    expect(sink.setDeckBuffer).toHaveBeenLastCalledWith(1, BUFFER_A)
    expect(reportStatus).toHaveBeenLastCalledWith(1, 'ready')
  })

  it('ignores a load that finishes after the track changed', async () => {
    const slow = deferred<AudioBuffer>()
    const { sync, reportStatus, sink } = setup({
      '/slow': slow.promise,
      '/fast': Promise.resolve(BUFFER_B)
    })
    sync(1, '/slow')
    sync(1, '/fast')
    await Promise.resolve()
    slow.resolve(BUFFER_A)
    await slow.promise

    expect(sink.setDeckBuffer).toHaveBeenLastCalledWith(1, BUFFER_B)
    expect(reportStatus).toHaveBeenLastCalledWith(1, 'ready')
  })

  it('warns and reports an error when loading fails', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {})
    const failing = Promise.reject(new Error('decode'))
    const { sync, reportStatus } = setup({ '/bad': failing })
    sync(2, '/bad')
    await failing.catch(() => {})
    expect(reportStatus).toHaveBeenLastCalledWith(2, 'error')
    expect(warn).toHaveBeenCalled()
    warn.mockRestore()
  })

  it('does nothing when asked for the url it already follows', () => {
    const load = vi.fn(() => Promise.resolve(BUFFER_A))
    const sink = { setDeckBuffer: vi.fn() }
    const { sync } = createDeckAudioSync(sink, { load }, vi.fn())
    sync(1, '/a')
    sync(1, '/a')
    expect(load).toHaveBeenCalledTimes(1)
  })
})
