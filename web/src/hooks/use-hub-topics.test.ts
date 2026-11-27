import { act, renderHook, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useHubTopics } from './use-hub-topics'

class FakeSocket {
  static instances: FakeSocket[] = []
  readonly listeners = new Map<string, ((event: Event) => void)[]>()
  readonly sent: string[] = []
  readonly url: string
  readyState = 0
  closed = false

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
    if (this.closed) return
    this.closed = true
    this.readyState = 3
    this.dispatch('close', new Event('close'))
  }

  open() {
    this.readyState = 1
    this.dispatch('open', new Event('open'))
  }

  dispatch(type: string, event: Event) {
    for (const listener of this.listeners.get(type) ?? []) listener(event)
  }
}

function hubEvent(topic: string, data: unknown): MessageEvent {
  return new MessageEvent('message', {
    data: JSON.stringify({ type: 'event', topic, data }),
  })
}

beforeEach(() => {
  FakeSocket.instances = []
  vi.stubGlobal('WebSocket', FakeSocket)
})

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('useHubTopics', () => {
  it('shares one socket between the hooks on a page and routes by topic', () => {
    const onAgents = vi.fn<() => void>()
    const onMetrics = vi.fn<() => void>()

    const agents = renderHook(() => useHubTopics(['agents'], { onEvent: onAgents }))
    const metrics = renderHook(() => useHubTopics(['system.metrics'], { onEvent: onMetrics }))

    expect(FakeSocket.instances).toHaveLength(1)
    const socket = FakeSocket.instances[0]
    if (!socket) return
    act(() => socket.open())

    expect(agents.result.current).toBe(true)
    expect(metrics.result.current).toBe(true)
    // The one socket asks for both topics.
    const subscribe = JSON.parse(socket.sent[0] ?? '{}') as { topics?: string[] }
    expect(subscribe.topics?.sort()).toEqual(['agents', 'system.metrics'])

    act(() => socket.dispatch('message', hubEvent('agents', { kind: 'claude' })))
    expect(onAgents).toHaveBeenCalledTimes(1)
    expect(onMetrics).not.toHaveBeenCalled()

    act(() => socket.dispatch('message', hubEvent('system.metrics', { sample: 1 })))
    expect(onMetrics).toHaveBeenCalledTimes(1)

    agents.unmount()
    expect(socket.closed).toBe(false)

    metrics.unmount()
    expect(socket.closed).toBe(true)
  })

  it('unsubscribes a topic when its last hook leaves', () => {
    const agents = renderHook(() => useHubTopics(['agents'], { onEvent: () => undefined }))
    renderHook(() => useHubTopics(['system.metrics'], { onEvent: () => undefined }))
    const socket = FakeSocket.instances[0]
    if (!socket) return
    act(() => socket.open())
    socket.sent.length = 0

    agents.unmount()

    const last = JSON.parse(socket.sent.at(-1) ?? '{}') as unknown
    expect(last).toEqual({ type: 'unsubscribe', topics: ['agents'] })
    expect(socket.closed).toBe(false)
  })

  it('reconnects once and tells every follower to refetch', async () => {
    const onAgents = vi.fn<() => void>()
    const onMetrics = vi.fn<() => void>()
    renderHook(() => useHubTopics(['agents'], { onEvent: onAgents, onReconnect: onAgents }))
    renderHook(() =>
      useHubTopics(['system.metrics'], { onEvent: onMetrics, onReconnect: onMetrics }),
    )

    const first = FakeSocket.instances[0]
    if (!first) return
    act(() => first.open())
    act(() => first.close())

    // The retry is a real timer, started at one second.
    await waitFor(() => expect(FakeSocket.instances).toHaveLength(2), { timeout: 2_500 })
    const second = FakeSocket.instances[1]
    if (!second) return
    act(() => second.open())

    expect(onAgents).toHaveBeenCalledTimes(1)
    expect(onMetrics).toHaveBeenCalledTimes(1)
  })

  it('does not call onReconnect for a hook that joins an open socket', () => {
    const onEvent = vi.fn<() => void>()
    const onReconnect = vi.fn<() => void>()
    renderHook(() => useHubTopics(['agents'], { onEvent }))
    const socket = FakeSocket.instances[0]
    if (!socket) return
    act(() => socket.open())

    const late = renderHook(() => useHubTopics(['system.metrics'], { onEvent, onReconnect }))

    expect(late.result.current).toBe(true)
    expect(onReconnect).not.toHaveBeenCalled()
    // The late hook asks the shared socket for its own topic.
    const last = JSON.parse(socket.sent[socket.sent.length - 1] ?? '{}') as { topics?: string[] }
    expect(last.topics).toEqual(['system.metrics'])
  })
})
