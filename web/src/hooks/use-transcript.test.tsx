import { act, renderHook, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { useTranscript } from './use-transcript'

class FakeSocket {
  static instances: FakeSocket[] = []
  sent: unknown[] = []
  private listeners = new Map<string, Set<(event: unknown) => void>>()

  readonly url: string
  readyState = 1

  constructor(url: string) {
    this.url = url
    FakeSocket.instances.push(this)
  }

  addEventListener(type: string, listener: (event: unknown) => void) {
    const set = this.listeners.get(type) ?? new Set()
    set.add(listener)
    this.listeners.set(type, set)
  }

  removeEventListener(type: string, listener: (event: unknown) => void) {
    this.listeners.get(type)?.delete(listener)
  }

  send(data: unknown) {
    this.sent.push(data)
  }

  close() {
    this.readyState = 3
  }

  emit(type: string, event: unknown) {
    for (const listener of this.listeners.get(type) ?? []) listener(event)
  }

  message(frame: unknown) {
    this.emit('message', { data: JSON.stringify(frame) })
  }
}

beforeEach(() => {
  FakeSocket.instances = []
  vi.stubGlobal('WebSocket', FakeSocket)
})

function socketAt(index = 0): FakeSocket {
  const socket = FakeSocket.instances[index]
  if (!socket) throw new Error(`no socket at ${index}`)
  return socket
}

describe('useTranscript', () => {
  it('loads the opening page and appends live records', async () => {
    const { result } = renderHook(() => useTranscript('session-1'))
    const socket = socketAt()
    expect(socket.url).toContain('/api/claude/session-1/transcript/ws')

    act(() => {
      socket.message({
        type: 'page',
        records: [{ role: 'user', blocks: [{ kind: 'text', text: 'hello' }] }],
        start: 0,
        hasEarlier: false,
      })
    })
    await waitFor(() => expect(result.current.loading).toBe(false))
    expect(result.current.records).toHaveLength(1)

    act(() => {
      socket.message({
        type: 'appended',
        records: [{ role: 'assistant', blocks: [{ kind: 'text', text: 'done' }] }],
      })
    })
    expect(result.current.records).toHaveLength(2)
    expect(result.current.records.map((record) => record.role)).toEqual(['user', 'assistant'])
  })

  it('asks for an earlier page once and prepends it', async () => {
    const { result } = renderHook(() => useTranscript('session-1'))
    const socket = socketAt()

    act(() => {
      socket.message({
        type: 'page',
        records: [{ role: 'assistant', blocks: [{ kind: 'text', text: 'newer' }] }],
        start: 10,
        hasEarlier: true,
      })
    })
    await waitFor(() => expect(result.current.hasEarlier).toBe(true))

    act(() => result.current.loadEarlier())
    act(() => result.current.loadEarlier())
    expect(socket.sent).toEqual([JSON.stringify({ type: 'earlier' })])

    act(() => {
      socket.message({
        type: 'earlier',
        records: [{ role: 'user', blocks: [{ kind: 'text', text: 'older' }] }],
        start: 0,
        hasEarlier: false,
      })
    })
    expect(result.current.records.map((record) => record.role)).toEqual(['user', 'assistant'])
    expect(result.current.hasEarlier).toBe(false)

    act(() => result.current.loadEarlier())
    expect(socket.sent).toHaveLength(2)
  })

  it('reports a handshake that never opens', async () => {
    const { result } = renderHook(() => useTranscript('session-1'))
    const socket = socketAt()

    act(() => socket.emit('close', {}))

    await waitFor(() =>
      expect(result.current.error).toBe('Cannot reach the transcript socket; retrying.'),
    )
    expect(result.current.loading).toBe(false)
  })

  it('reports a protocol error and reads subagents', async () => {
    const { result } = renderHook(() => useTranscript('session-1'))
    const socket = socketAt()

    act(() => {
      socket.message({
        type: 'subagents',
        items: [{ agent: 'agent-a1', toolUseId: 'toolu_x', finished: true }],
      })
    })
    expect(result.current.subagents.map((item) => item.agent)).toEqual(['agent-a1'])

    act(() => socket.message({ type: 'error', message: 'cannot read the transcript' }))
    await waitFor(() => expect(result.current.error).toBe('cannot read the transcript'))
  })

  it('realigns with the server when the record cap would be exceeded', async () => {
    const { result } = renderHook(() => useTranscript('session-1'))
    const socket = socketAt()
    act(() => {
      socket.message({
        type: 'page',
        records: [{ role: 'user', blocks: [{ kind: 'text', text: 'seed' }] }],
        start: 0,
        hasEarlier: true,
      })
    })
    await waitFor(() => expect(result.current.records).toHaveLength(1))

    act(() => {
      socket.message({
        type: 'appended',
        records: Array.from({ length: 501 }, () => ({
          role: 'user',
          blocks: [{ kind: 'text', text: 'bulk' }],
        })),
      })
    })

    expect(socket.readyState).toBe(3)
    expect(result.current.records).toHaveLength(1)
  })

  it('closes the old socket and keeps the new one addressable', () => {
    const { result, rerender } = renderHook(({ id }: { id: string }) => useTranscript(id), {
      initialProps: { id: 'a' },
    })
    const first = socketAt(0)

    rerender({ id: 'b' })

    expect(first.readyState).toBe(3)
    const second = socketAt(1)
    // A late close from the old socket must not clear the new one.
    act(() => first.emit('close', {}))
    act(() => result.current.loadEarlier())
    expect(second.sent).toEqual([JSON.stringify({ type: 'earlier' })])
  })

  it('clears an in-flight earlier request when the target changes', () => {
    const { result, rerender } = renderHook(({ id }: { id: string }) => useTranscript(id), {
      initialProps: { id: 'a' },
    })
    const first = socketAt(0)
    act(() => {
      first.message({
        type: 'page',
        records: [{ role: 'user', blocks: [{ kind: 'text', text: 'seed' }] }],
        start: 10,
        hasEarlier: true,
      })
    })
    act(() => result.current.loadEarlier())
    expect(first.sent).toHaveLength(1)

    rerender({ id: 'b' })
    const second = socketAt(1)

    act(() => result.current.loadEarlier())
    expect(second.sent).toEqual([JSON.stringify({ type: 'earlier' })])
  })

  it('does not reconnect when earlier pages push past the cap', async () => {
    const { result } = renderHook(() => useTranscript('session-1'))
    const socket = socketAt()
    const record = (text: string) => ({
      role: 'user' as const,
      blocks: [{ kind: 'text' as const, text }],
    })

    act(() => {
      socket.message({ type: 'page', records: [record('seed')], start: 10, hasEarlier: true })
    })
    await waitFor(() => expect(result.current.records).toHaveLength(1))

    act(() => {
      socket.message({
        type: 'earlier',
        records: Array.from({ length: 600 }, (_, index) => record(`older ${index}`)),
        start: 0,
        hasEarlier: false,
      })
    })
    act(() => socket.message({ type: 'appended', records: [record('live')] }))

    expect(socket.readyState).toBe(1)
    expect(result.current.records).toHaveLength(602)
  })

  it('does nothing without a session id', () => {
    const { result } = renderHook(() => useTranscript(null))

    expect(FakeSocket.instances).toHaveLength(0)
    expect(result.current.loading).toBe(false)
  })
})

describe('useTranscript reconnection', () => {
  it('reconnects after a dropped socket', async () => {
    vi.useFakeTimers()
    try {
      renderHook(() => useTranscript('session-1'))
      const first = socketAt()

      act(() => first.emit('close', {}))
      act(() => {
        vi.advanceTimersByTime(1000)
      })

      expect(FakeSocket.instances).toHaveLength(2)
    } finally {
      vi.useRealTimers()
    }
  })

  it('stops after a fatal error frame', () => {
    vi.useFakeTimers()
    try {
      renderHook(() => useTranscript('session-1'))
      const socket = socketAt()

      act(() => socket.message({ type: 'error', message: 'the transcript went away' }))
      act(() => socket.emit('close', {}))
      act(() => {
        vi.advanceTimersByTime(60_000)
      })

      expect(FakeSocket.instances).toHaveLength(1)
    } finally {
      vi.useRealTimers()
    }
  })
})
