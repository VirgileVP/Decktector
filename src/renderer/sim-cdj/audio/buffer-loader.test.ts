import { describe, expect, it, vi } from 'vitest'

import { createBufferLoader } from './buffer-loader'

const BUFFER = { duration: 1 } as AudioBuffer

function okResponse(): Response {
  return { ok: true, status: 200, arrayBuffer: async () => new ArrayBuffer(4) } as Response
}

describe('createBufferLoader', () => {
  it('decodes a URL once however often it is asked for', async () => {
    const fetchAudio = vi.fn(async () => okResponse())
    const decode = vi.fn(async () => BUFFER)
    const loader = createBufferLoader(decode, fetchAudio as unknown as typeof fetch)

    const [first, second] = await Promise.all([loader.load('/audio/1'), loader.load('/audio/1')])
    await loader.load('/audio/1')

    expect(first).toBe(BUFFER)
    expect(second).toBe(BUFFER)
    expect(fetchAudio).toHaveBeenCalledTimes(1)
    expect(decode).toHaveBeenCalledTimes(1)
  })

  it('rejects on an HTTP error and tries again next time', async () => {
    const fetchAudio = vi
      .fn()
      .mockResolvedValueOnce({ ok: false, status: 404 } as Response)
      .mockResolvedValueOnce(okResponse())
    const loader = createBufferLoader(async () => BUFFER, fetchAudio as unknown as typeof fetch)

    await expect(loader.load('/audio/9')).rejects.toThrow('404')
    await expect(loader.load('/audio/9')).resolves.toBe(BUFFER)
  })

  it('rejects when decoding fails and does not cache the failure', async () => {
    const decode = vi
      .fn()
      .mockRejectedValueOnce(new Error('bad data'))
      .mockResolvedValueOnce(BUFFER)
    const loader = createBufferLoader(decode, (async () => okResponse()) as unknown as typeof fetch)

    await expect(loader.load('/audio/3')).rejects.toThrow('bad data')
    await expect(loader.load('/audio/3')).resolves.toBe(BUFFER)
  })

  it('evicts the least recently used entry past its capacity', async () => {
    const fetchAudio = vi.fn(async () => okResponse())
    const loader = createBufferLoader(async () => BUFFER, fetchAudio as unknown as typeof fetch, 2)

    await loader.load('/audio/1')
    await loader.load('/audio/2')
    await loader.load('/audio/1') // 2 is now the oldest
    await loader.load('/audio/3') // evicts 2
    await loader.load('/audio/1')
    expect(fetchAudio).toHaveBeenCalledTimes(3)
    await loader.load('/audio/2')
    expect(fetchAudio).toHaveBeenCalledTimes(4)
  })
})
