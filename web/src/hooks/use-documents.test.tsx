import { renderHook, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { DocumentsSnapshot } from '@/lib/documents'
import { useDocuments } from './use-documents'

const { get } = vi.hoisted(() => ({
  get: vi.fn<(path: string, options?: unknown) => Promise<{ data?: unknown; error?: unknown }>>(),
}))

vi.mock('@/api/client', () => ({ api: { GET: get } }))

const snapshot: DocumentsSnapshot = {
  collections: [
    {
      id: 'probe-visuals',
      name: 'Probe visuals',
      icon: 'chart',
      root: '~/notes',
      groups: [],
    },
  ],
  warnings: [{ file: 'broken.json', problem: 'not valid JSON' }],
}

beforeEach(() => {
  get.mockReset()
})

describe('useDocuments', () => {
  it('loads the snapshot', async () => {
    get.mockResolvedValue({ data: snapshot })

    const { result } = renderHook(() => useDocuments())

    expect(result.current.loading).toBe(true)
    await waitFor(() => expect(result.current.loading).toBe(false))
    expect(result.current.snapshot).toEqual(snapshot)
    expect(result.current.error).toBeNull()
    expect(get).toHaveBeenCalledWith('/api/documents')
  })

  it('reports a failed snapshot', async () => {
    get.mockResolvedValue({ error: { error: 'internal', message: 'the listing failed' } })

    const { result } = renderHook(() => useDocuments())

    await waitFor(() => expect(result.current.loading).toBe(false))
    expect(result.current.error).toBe('the listing failed')
    expect(result.current.snapshot).toBeNull()
  })

  it('fetches again on refresh', async () => {
    get.mockResolvedValue({ data: snapshot })

    const { result } = renderHook(() => useDocuments())
    await waitFor(() => expect(result.current.loading).toBe(false))
    get.mockClear()

    result.current.refresh()

    await waitFor(() => expect(get).toHaveBeenCalledTimes(1))
    expect(result.current.loading).toBe(false)
  })
})
