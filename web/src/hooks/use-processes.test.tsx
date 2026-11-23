import { act, renderHook, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { ProcessInfo } from '@/lib/processes'
import { useProcesses } from './use-processes'

const { get, post } = vi.hoisted(() => ({
  get: vi.fn<(path: string) => Promise<{ data?: unknown; error?: unknown }>>(),
  post: vi.fn<(path: string, options?: unknown) => Promise<{ error?: unknown }>>(),
}))

vi.mock('@/api/client', () => ({ api: { GET: get, POST: post } }))

const node: ProcessInfo = {
  pid: 812,
  parent_pid: 1,
  name: 'node',
  user: 'dev',
  state: 'running',
  cpu_seconds: 12,
  memory_bytes: 2048,
  command: 'node vite',
  started_at_unix: 1_700_000_000,
  niceness: 0,
}

beforeEach(() => {
  get.mockReset()
  post.mockReset()
})

describe('useProcesses', () => {
  it('loads the process table', async () => {
    get.mockResolvedValue({ data: [node] })

    const { result } = renderHook(() => useProcesses())

    expect(result.current.loading).toBe(true)
    await waitFor(() => expect(result.current.loading).toBe(false))
    expect(result.current.processes).toEqual([node])
    expect(result.current.error).toBeNull()
    expect(get).toHaveBeenCalledWith('/api/processes')
  })

  it('reports a failed load', async () => {
    get.mockResolvedValue({ error: { error: 'internal', message: 'an internal error occurred' } })

    const { result } = renderHook(() => useProcesses())

    await waitFor(() => expect(result.current.loading).toBe(false))
    expect(result.current.error).toBe('an internal error occurred')
    expect(result.current.processes).toEqual([])
  })

  it('sends a signal and reloads the table', async () => {
    get.mockResolvedValue({ data: [node] })
    post.mockResolvedValue({})

    const { result } = renderHook(() => useProcesses())
    await waitFor(() => expect(result.current.loading).toBe(false))
    get.mockClear()

    await act(async () => {
      await result.current.signal(812, 'term')
    })

    expect(post).toHaveBeenCalledWith('/api/processes/signal', {
      body: { pid: 812, signal: 'term' },
    })
    await waitFor(() => expect(get).toHaveBeenCalledTimes(1))
    expect(result.current.busyPid).toBeNull()
  })

  it('renices a process and reloads the table', async () => {
    get.mockResolvedValue({ data: [node] })
    post.mockResolvedValue({})

    const { result } = renderHook(() => useProcesses())
    await waitFor(() => expect(result.current.loading).toBe(false))
    get.mockClear()

    await act(async () => {
      await result.current.renice(812, 10)
    })

    expect(post).toHaveBeenCalledWith('/api/processes/renice', {
      body: { pid: 812, priority: 10 },
    })
    await waitFor(() => expect(get).toHaveBeenCalledTimes(1))
  })

  it('shows the server message when an action is refused and does not reload', async () => {
    get.mockResolvedValue({ data: [node] })
    post.mockResolvedValue({
      error: { error: 'permission_denied', message: 'permission denied for process 812' },
    })

    const { result } = renderHook(() => useProcesses())
    await waitFor(() => expect(result.current.loading).toBe(false))
    get.mockClear()

    await act(async () => {
      await result.current.signal(812, 'kill')
    })

    expect(result.current.error).toBe('permission denied for process 812')
    expect(result.current.busyPid).toBeNull()
    expect(get).not.toHaveBeenCalled()
  })

  it('reports a thrown request as a message', async () => {
    get.mockResolvedValue({ data: [] })
    post.mockRejectedValue(new Error('network down'))

    const { result } = renderHook(() => useProcesses())
    await waitFor(() => expect(result.current.loading).toBe(false))

    await act(async () => {
      await result.current.renice(1, 0)
    })

    expect(result.current.error).toBe('network down')
  })
})
