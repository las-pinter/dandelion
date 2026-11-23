import { act, renderHook, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { HOUR_MS, type MetricsPoint, type MetricsRange, type Snapshot } from '@/lib/metrics'
import { useSystemMetrics } from './use-system-metrics'

const { get } = vi.hoisted(() => ({
  get: vi.fn<(path: string, options?: unknown) => Promise<{ data?: unknown; error?: unknown }>>(),
}))

vi.mock('@/api/client', () => ({ api: { GET: get } }))

function snapshot(timestamp_ms: number, cpu_percent = 25): Snapshot {
  return {
    timestamp_ms,
    uptime_secs: 10,
    cpu: {
      usage_percent: cpu_percent,
      per_core_percent: [cpu_percent],
      load: { one: 0.5, five: 0.4, fifteen: 0.3 },
    },
    memory: {
      total_bytes: 8,
      used_bytes: 2,
      available_bytes: 6,
      cache_bytes: 0,
      swap_total_bytes: 0,
      swap_used_bytes: 0,
    },
    disks: [],
    network: [],
    temperatures: [],
    top_processes_by_cpu: [],
    top_processes_by_memory: [],
  }
}

function point(timestamp_ms: number, cpu_percent = 25): MetricsPoint {
  return {
    timestamp_ms,
    cpu_percent,
    memory_used_bytes: 2,
    memory_total_bytes: 8,
    load1: 0.5,
    rx_bytes_per_sec: 0,
    tx_bytes_per_sec: 0,
    read_bytes_per_sec: 0,
    write_bytes_per_sec: 0,
  }
}

class FakeSocket {
  static instances: FakeSocket[] = []
  readonly listeners = new Map<string, ((event: Event) => void)[]>()
  readonly sent: string[] = []
  readonly url: string

  constructor(url: string) {
    this.url = url
    FakeSocket.instances.push(this)
  }

  addEventListener(type: string, listener: (event: Event) => void) {
    const listeners = this.listeners.get(type) ?? []
    listeners.push(listener)
    this.listeners.set(type, listeners)
  }

  send(data: string) {
    this.sent.push(data)
  }

  close() {
    this.dispatch('close', new Event('close'))
  }

  dispatch(type: string, event: Event) {
    for (const listener of this.listeners.get(type) ?? []) listener(event)
  }
}

beforeEach(() => {
  FakeSocket.instances = []
  vi.stubGlobal('WebSocket', FakeSocket)
  get.mockImplementation((path: string) => {
    if (path === '/api/system/latest') {
      return Promise.resolve({ data: snapshot(1), error: undefined })
    }
    return Promise.resolve({
      data: { seconds: [point(1)], minutes: [] },
      error: undefined,
    })
  })
})

afterEach(() => {
  vi.unstubAllGlobals()
  get.mockReset()
})

describe('useSystemMetrics', () => {
  it('loads history and applies a live sample', async () => {
    const { result, unmount } = renderHook(() => useSystemMetrics('1h'))

    await waitFor(() => expect(get).toHaveBeenCalledTimes(2))
    await waitFor(() => expect(result.current.snapshot).not.toBeNull())

    const socket = FakeSocket.instances[0]
    expect(socket).toBeDefined()
    if (!socket) return

    act(() => socket.dispatch('open', new Event('open')))
    expect(socket.sent[0]).toContain('system.metrics')
    expect(result.current.connected).toBe(true)

    act(() =>
      socket.dispatch(
        'message',
        new MessageEvent('message', {
          data: JSON.stringify({
            type: 'event',
            topic: 'system.metrics',
            data: { snapshot: snapshot(2, 55), point: point(2) },
          }),
        }),
      ),
    )

    expect(result.current.snapshot?.cpu.usage_percent).toBe(55)
    expect(result.current.seconds.at(-1)?.timestamp_ms).toBe(2)
    unmount()
  })

  it('trims the live hour series to its window', async () => {
    const { result } = renderHook(() => useSystemMetrics('1h'))
    await waitFor(() => expect(result.current.snapshot).not.toBeNull())

    const socket = FakeSocket.instances[0]
    if (!socket) return
    act(() => socket.dispatch('open', new Event('open')))

    const future = HOUR_MS + 2
    act(() =>
      socket.dispatch(
        'message',
        new MessageEvent('message', {
          data: JSON.stringify({
            type: 'event',
            topic: 'system.metrics',
            data: { snapshot: snapshot(future), point: point(future) },
          }),
        }),
      ),
    )

    expect(result.current.seconds.map((entry) => entry.timestamp_ms)).toEqual([future])
  })

  it('ignores a malformed live payload', async () => {
    const { result } = renderHook(() => useSystemMetrics('1h'))
    await waitFor(() => expect(result.current.snapshot?.timestamp_ms).toBe(1))

    const socket = FakeSocket.instances[0]
    if (!socket) return
    act(() => socket.dispatch('open', new Event('open')))
    act(() =>
      socket.dispatch(
        'message',
        new MessageEvent('message', {
          data: JSON.stringify({
            type: 'event',
            topic: 'system.metrics',
            data: { nonsense: true },
          }),
        }),
      ),
    )

    expect(result.current.snapshot?.timestamp_ms).toBe(1)
  })

  it('refetches history after a reconnect', async () => {
    const { result } = renderHook(() => useSystemMetrics('1h'))
    await waitFor(() => expect(get).toHaveBeenCalledTimes(2))

    const first = FakeSocket.instances[0]
    if (!first) return
    act(() => first.close())

    await waitFor(() => expect(FakeSocket.instances.length).toBe(2), { timeout: 2_500 })
    const second = FakeSocket.instances[1]
    if (!second) return
    act(() => second.dispatch('open', new Event('open')))

    await waitFor(() => expect(get).toHaveBeenCalledTimes(4))
    expect(result.current.connected).toBe(true)
  })

  it('fetches the new window when the range changes', async () => {
    const { rerender } = renderHook(
      ({ range }: { range: MetricsRange }) => useSystemMetrics(range),
      { initialProps: { range: '1h' as MetricsRange } },
    )

    await waitFor(() =>
      expect(get).toHaveBeenCalledWith('/api/system/history', {
        params: { query: { range: '1h' } },
      }),
    )

    rerender({ range: '7d' })

    await waitFor(() =>
      expect(get).toHaveBeenCalledWith('/api/system/history', {
        params: { query: { range: '7d' } },
      }),
    )
  })

  it('reports an unavailable history', async () => {
    get.mockImplementation((path: string) =>
      path === '/api/system/latest'
        ? Promise.resolve({ data: snapshot(1), error: undefined })
        : Promise.resolve({ data: undefined, error: { error: 'internal' } }),
    )

    const { result } = renderHook(() => useSystemMetrics('1h'))

    await waitFor(() => expect(result.current.error).toBe('history unavailable'))
  })
})
