import { render, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { TerminalView } from './TerminalView'

const mocks = vi.hoisted(() => ({
  writes: [] as unknown[],
  fitted: 0,
  disposed: 0,
  del: vi.fn<(path: string, options?: unknown) => Promise<{ error?: unknown }>>(async () => ({})),
}))

vi.mock('@/api/client', () => ({ api: { DELETE: mocks.del } }))

vi.mock('@xterm/xterm', () => ({
  Terminal: class {
    cols = 80
    rows = 24
    open() {}
    loadAddon() {}
    onData() {
      return { dispose: () => undefined }
    }
    write(data: unknown) {
      mocks.writes.push(data)
    }
    dispose() {
      mocks.disposed += 1
    }
  },
}))

vi.mock('@xterm/addon-fit', () => ({
  FitAddon: class {
    fit() {
      mocks.fitted += 1
    }
  },
}))

class FakeSocket {
  static OPEN = 1
  static instances: FakeSocket[] = []
  readonly url: string
  readyState = 0
  sent: unknown[] = []
  private listeners = new Map<string, Set<(event: unknown) => void>>()

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
    if (this.readyState === 3) return
    this.readyState = 3
    this.emit('close', {})
  }

  emit(type: string, event: unknown) {
    for (const listener of this.listeners.get(type) ?? []) listener(event)
  }
}

/** The view defers its socket by one tick; tests wait for it to exist. */
async function socketOf(): Promise<FakeSocket> {
  await waitFor(() => expect(FakeSocket.instances).toHaveLength(1))
  const socket = FakeSocket.instances[0]
  expect(socket).toBeDefined()
  return socket as FakeSocket
}

beforeEach(() => {
  mocks.writes = []
  mocks.fitted = 0
  mocks.disposed = 0
  mocks.del.mockClear()
  FakeSocket.instances = []
  vi.stubGlobal('WebSocket', FakeSocket)
  vi.stubGlobal(
    'ResizeObserver',
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  )
})

describe('TerminalView', () => {
  it('connects the socket for its terminal and asks for a size', async () => {
    render(<TerminalView id="abc" />)
    const socket = await socketOf()

    expect(socket.url).toBe('ws://localhost:3000/api/terminals/abc/ws')

    socket.emit('open', {})
    expect(socket.sent).toContain('{"type":"resize","cols":80,"rows":24}')
  })

  it('writes binary output and ignores text frames', async () => {
    render(<TerminalView id="abc" />)
    const socket = await socketOf()

    socket.emit('message', { data: new Uint8Array([104, 101, 108, 108, 111]).buffer })
    socket.emit('message', { data: 'not binary' })

    expect(mocks.writes).toHaveLength(1)
    expect(new TextDecoder().decode(mocks.writes[0] as Uint8Array)).toBe('hello')
  })

  it('closes the socket and disposes the view on unmount', async () => {
    const { unmount } = render(<TerminalView id="abc" />)
    const socket = await socketOf()
    socket.emit('open', {})

    unmount()

    expect(socket.readyState).toBe(3)
    expect(mocks.disposed).toBe(1)
    expect(mocks.del).not.toHaveBeenCalled()
  })

  it('deletes a shell whose socket never opened', async () => {
    const { unmount } = render(<TerminalView id="abc" />)
    await socketOf()

    unmount()

    expect(mocks.del).toHaveBeenCalledWith('/api/terminals/{id}', {
      params: { path: { id: 'abc' } },
    })
  })

  it('deletes when the socket closes before opening', async () => {
    render(<TerminalView id="abc" />)
    const socket = await socketOf()

    socket.emit('close', {})

    expect(mocks.del).toHaveBeenCalledTimes(1)
  })

  it('never writes to a disposed view', async () => {
    const { unmount } = render(<TerminalView id="abc" />)
    const socket = await socketOf()
    socket.emit('open', {})
    mocks.writes = []

    unmount()
    socket.emit('message', { data: new Uint8Array([1]).buffer })

    expect(mocks.writes).toHaveLength(0)
  })

  it('opens no socket when it unmounts in the same tick', () => {
    const { unmount } = render(<TerminalView id="abc" />)

    unmount()

    expect(FakeSocket.instances).toHaveLength(0)
    expect(mocks.del).not.toHaveBeenCalled()
  })
})
