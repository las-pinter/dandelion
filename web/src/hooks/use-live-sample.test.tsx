import { act, renderHook } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { useLiveSample } from './use-live-sample'

class FakeSocket {
  static instances: FakeSocket[] = []
  private listeners = new Map<string, Set<(event: unknown) => void>>()
  readyState = 1

  constructor() {
    FakeSocket.instances.push(this)
  }

  addEventListener(type: string, listener: (event: unknown) => void) {
    const set = this.listeners.get(type) ?? new Set()
    set.add(listener)
    this.listeners.set(type, set)
  }

  send() {}
  close() {}

  emit(type: string, event: unknown) {
    for (const listener of this.listeners.get(type) ?? []) listener(event)
  }
}

function snapshot(timestamp_ms: number, cores: number) {
  return {
    timestamp_ms,
    uptime_secs: 10,
    cpu: {
      usage_percent: 4,
      per_core_percent: Array.from({ length: cores }, () => 4),
      load: { one: 0.1, five: 0.1, fifteen: 0.1 },
    },
    memory: {
      total_bytes: 8,
      used_bytes: 4,
      available_bytes: 4,
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

/** One hub frame for the metrics topic. */
function live(sample: ReturnType<typeof snapshot>, at: number) {
  return {
    data: JSON.stringify({
      type: 'event',
      topic: 'system.metrics',
      data: { snapshot: sample, point: { timestamp_ms: at } },
    }),
  }
}

beforeEach(() => {
  FakeSocket.instances = []
  vi.stubGlobal('WebSocket', FakeSocket)
})

describe('useLiveSample', () => {
  it('fills from the live topic and reports the socket', () => {
    const { result } = renderHook(() => useLiveSample())
    const socket = FakeSocket.instances[0]
    if (!socket) return

    expect(result.current.snapshot).toBeNull()
    act(() => socket.emit('open', {}))
    expect(result.current.connected).toBe(true)

    act(() => socket.emit('message', live(snapshot(5, 10), 5)))

    expect(result.current.snapshot?.cpu.per_core_percent).toHaveLength(10)
  })

  it('ignores a sample older than the one it holds', () => {
    const { result } = renderHook(() => useLiveSample())
    const socket = FakeSocket.instances[0]
    if (!socket) return

    act(() => socket.emit('message', live(snapshot(10, 8), 10)))
    act(() => socket.emit('message', live(snapshot(4, 2), 4)))

    expect(result.current.snapshot?.cpu.per_core_percent).toHaveLength(8)
  })

  it('ignores an event on another topic', () => {
    const { result } = renderHook(() => useLiveSample())
    const socket = FakeSocket.instances[0]
    if (!socket) return

    act(() =>
      socket.emit('message', {
        data: JSON.stringify({
          type: 'event',
          topic: 'agents',
          data: { snapshot: snapshot(5, 10), point: { timestamp_ms: 5 } },
        }),
      }),
    )

    expect(result.current.snapshot).toBeNull()
  })
})
