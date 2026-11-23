import { act, renderHook, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { JournalEntry, ServicesSnapshot } from '@/lib/services'
import { useServices } from './use-services'

const { get, post } = vi.hoisted(() => ({
  get: vi.fn<(path: string, options?: unknown) => Promise<{ data?: unknown; error?: unknown }>>(),
  post: vi.fn<(path: string, options?: unknown) => Promise<{ error?: unknown }>>(),
}))

vi.mock('@/api/client', () => ({ api: { GET: get, POST: post } }))

const snapshot: ServicesSnapshot = {
  units: [
    {
      name: 'nginx.service',
      description: 'A high performance web server',
      load_state: 'loaded',
      active_state: 'active',
      sub_state: 'running',
      scope: 'system',
    },
  ],
  allowlist: [{ unit: 'nginx.service', verbs: ['restart'] }],
}

const entries: JournalEntry[] = [
  {
    timestamp_usec: 1_700_000_000_000_000,
    message: 'started',
    priority: 6,
    unit: 'nginx.service',
    identifier: 'nginx',
    pid: 42,
  },
]

beforeEach(() => {
  get.mockReset()
  post.mockReset()
})

describe('useServices', () => {
  it('loads the snapshot', async () => {
    get.mockResolvedValue({ data: snapshot })

    const { result } = renderHook(() => useServices())

    expect(result.current.loading).toBe(true)
    await waitFor(() => expect(result.current.loading).toBe(false))
    expect(result.current.snapshot).toEqual(snapshot)
    expect(result.current.error).toBeNull()
    expect(get).toHaveBeenCalledWith('/api/services')
  })

  it('reports a failed snapshot', async () => {
    get.mockResolvedValue({
      error: { error: 'unavailable', message: 'systemd is unavailable to the dandelion server' },
    })

    const { result } = renderHook(() => useServices())

    await waitFor(() => expect(result.current.loading).toBe(false))
    expect(result.current.error).toBe('systemd is unavailable to the dandelion server')
    expect(result.current.snapshot).toBeNull()
  })

  it('sends an action and reloads the units', async () => {
    get.mockResolvedValue({ data: snapshot })
    post.mockResolvedValue({})

    const { result } = renderHook(() => useServices())
    await waitFor(() => expect(result.current.loading).toBe(false))
    get.mockClear()

    await act(async () => {
      await result.current.act('system', 'nginx.service', 'restart')
    })

    expect(post).toHaveBeenCalledWith('/api/services/action', {
      body: { scope: 'system', unit: 'nginx.service', action: 'restart' },
    })
    await waitFor(() => expect(get).toHaveBeenCalledTimes(1))
    expect(result.current.busy).toBeNull()
  })

  it('shows a refused action and does not reload', async () => {
    get.mockResolvedValue({ data: snapshot })
    post.mockResolvedValue({
      error: { error: 'not_allowed', message: 'unit `nginx.service` is not in the sudo allowlist' },
    })

    const { result } = renderHook(() => useServices())
    await waitFor(() => expect(result.current.loading).toBe(false))
    get.mockClear()

    await act(async () => {
      await result.current.act('system', 'nginx.service', 'stop')
    })

    expect(result.current.error).toContain('not in the sudo allowlist')
    expect(get).not.toHaveBeenCalled()
  })

  it('loads one journal tail', async () => {
    get.mockImplementation(async (path) =>
      path === '/api/services' ? { data: snapshot } : { data: entries },
    )

    const { result } = renderHook(() => useServices())
    await waitFor(() => expect(result.current.loading).toBe(false))

    await act(async () => {
      await result.current.loadJournal('system', 'nginx.service')
    })

    expect(get).toHaveBeenCalledWith('/api/services/{scope}/{unit}/journal', {
      params: { path: { scope: 'system', unit: 'nginx.service' }, query: { lines: 200 } },
    })
    expect(result.current.journal).toEqual(entries)
    expect(result.current.journalLoading).toBe(false)
  })

  it('reports a failed journal and clears the entries', async () => {
    get.mockImplementation(async (path) =>
      path === '/api/services' ? { data: snapshot } : { error: { message: 'journal unavailable' } },
    )

    const { result } = renderHook(() => useServices())
    await waitFor(() => expect(result.current.loading).toBe(false))

    await act(async () => {
      await result.current.loadJournal('user', 'app.service')
    })

    expect(result.current.journal).toEqual([])
    expect(result.current.journalError).toBe('journal unavailable')
  })

  it('ignores a journal response that a newer selection replaced', async () => {
    get.mockResolvedValue({ data: snapshot })
    const { result } = renderHook(() => useServices())
    await waitFor(() => expect(result.current.loading).toBe(false))

    const slow = deferred<{ data: JournalEntry[] }>()
    const fast = deferred<{ data: JournalEntry[] }>()
    get.mockImplementationOnce(() => slow.promise)
    get.mockImplementationOnce(() => fast.promise)

    let first!: Promise<void>
    let second!: Promise<void>
    await act(async () => {
      first = result.current.loadJournal('user', 'a.service')
      second = result.current.loadJournal('user', 'b.service')
    })

    await act(async () => {
      fast.resolve({ data: entries })
      await second
    })
    await act(async () => {
      slow.resolve({
        data: [{ ...entries[0], message: 'stale entry' }],
      })
      await first
    })

    expect(result.current.journal).toEqual(entries)
    expect(result.current.journalLoading).toBe(false)
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
