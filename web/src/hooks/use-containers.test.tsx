import { act, renderHook, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { ContainerLogLine, ContainersSnapshot } from '@/lib/containers'
import { useContainers } from './use-containers'

const { get, post } = vi.hoisted(() => ({
  get: vi.fn<(path: string, options?: unknown) => Promise<{ data?: unknown; error?: unknown }>>(),
  post: vi.fn<(path: string, options?: unknown) => Promise<{ error?: unknown }>>(),
}))

vi.mock('@/api/client', () => ({ api: { GET: get, POST: post } }))

const snapshot: ContainersSnapshot = {
  availability: { available: true, reason: null, server_version: '29.6.2' },
  containers: [
    {
      id: 'abc123def456',
      name: 'web',
      image: 'nginx:1',
      command: 'nginx -g daemon off;',
      state: 'running',
      status: 'Up 2 minutes',
      created_at_unix: 1_700_000_000,
      labels: {},
    },
  ],
}

const lines: ContainerLogLine[] = [
  { stream: 'stdout', message: '2026-11-23T12:34:56Z listening on :80' },
]

beforeEach(() => {
  get.mockReset()
  post.mockReset()
})

describe('useContainers', () => {
  it('loads the snapshot', async () => {
    get.mockResolvedValue({ data: snapshot })

    const { result } = renderHook(() => useContainers())

    expect(result.current.loading).toBe(true)
    await waitFor(() => expect(result.current.loading).toBe(false))
    expect(result.current.snapshot).toEqual(snapshot)
    expect(result.current.error).toBeNull()
    expect(get).toHaveBeenCalledWith('/api/containers')
  })

  it('reports a failed snapshot', async () => {
    get.mockResolvedValue({
      error: { error: 'docker_error', message: 'Docker API error: boom' },
    })

    const { result } = renderHook(() => useContainers())

    await waitFor(() => expect(result.current.loading).toBe(false))
    expect(result.current.error).toBe('Docker API error: boom')
    expect(result.current.snapshot).toBeNull()
  })

  it('sends an action and reloads the containers', async () => {
    get.mockResolvedValue({ data: snapshot })
    post.mockResolvedValue({})

    const { result } = renderHook(() => useContainers())
    await waitFor(() => expect(result.current.loading).toBe(false))
    get.mockClear()

    await act(async () => {
      await result.current.act('web', 'restart')
    })

    expect(post).toHaveBeenCalledWith('/api/containers/action', {
      body: { container: 'web', action: 'restart' },
    })
    await waitFor(() => expect(get).toHaveBeenCalledTimes(1))
    expect(result.current.busy).toBeNull()
  })

  it('shows a refused action and does not reload', async () => {
    get.mockResolvedValue({ data: snapshot })
    post.mockResolvedValue({
      error: { error: 'invalid_container', message: 'invalid container reference `-web`' },
    })

    const { result } = renderHook(() => useContainers())
    await waitFor(() => expect(result.current.loading).toBe(false))
    get.mockClear()

    await act(async () => {
      await result.current.act('-web', 'start')
    })

    expect(result.current.error).toContain('invalid container reference')
    expect(get).not.toHaveBeenCalled()
  })

  it('loads one log tail', async () => {
    get.mockImplementation(async (path) =>
      path === '/api/containers' ? { data: snapshot } : { data: lines },
    )

    const { result } = renderHook(() => useContainers())
    await waitFor(() => expect(result.current.loading).toBe(false))

    await act(async () => {
      await result.current.loadLogs('web')
    })

    expect(get).toHaveBeenCalledWith('/api/containers/{container}/logs', {
      params: { path: { container: 'web' }, query: { tail: 200 } },
    })
    expect(result.current.logs).toEqual(lines)
    expect(result.current.logsLoading).toBe(false)
  })

  it('reports a failed log tail and clears the lines', async () => {
    get.mockImplementation(async (path) =>
      path === '/api/containers'
        ? { data: snapshot }
        : { error: { message: 'Docker is unavailable' } },
    )

    const { result } = renderHook(() => useContainers())
    await waitFor(() => expect(result.current.loading).toBe(false))

    await act(async () => {
      await result.current.loadLogs('web')
    })

    expect(result.current.logs).toEqual([])
    expect(result.current.logsError).toBe('Docker is unavailable')
  })

  it('ignores a log response that a newer selection replaced', async () => {
    get.mockResolvedValue({ data: snapshot })
    const { result } = renderHook(() => useContainers())
    await waitFor(() => expect(result.current.loading).toBe(false))

    const slow = deferred<{ data: ContainerLogLine[] }>()
    const fast = deferred<{ data: ContainerLogLine[] }>()
    get.mockImplementationOnce(() => slow.promise)
    get.mockImplementationOnce(() => fast.promise)

    let first!: Promise<void>
    let second!: Promise<void>
    await act(async () => {
      first = result.current.loadLogs('web')
      second = result.current.loadLogs('db')
    })

    await act(async () => {
      fast.resolve({ data: lines })
      await second
    })
    await act(async () => {
      slow.resolve({
        data: [{ stream: 'stderr', message: 'stale line' }],
      })
      await first
    })

    expect(result.current.logs).toEqual(lines)
    expect(result.current.logsLoading).toBe(false)
  })
})

/** A promise a test resolves by hand. */
function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((done) => {
    resolve = done
  })
  return { promise, resolve }
}
