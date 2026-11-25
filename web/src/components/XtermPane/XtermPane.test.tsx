import { render, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { XtermPane } from './XtermPane'

const mocks = vi.hoisted(() => ({ disposed: 0 }))

vi.mock('@xterm/xterm', () => ({
  Terminal: class {
    cols = 80
    rows = 24
    open() {}
    loadAddon() {}
    onData() {
      return { dispose: () => undefined }
    }
    write() {}
    dispose() {
      mocks.disposed += 1
    }
  },
}))

vi.mock('@xterm/addon-fit', () => ({
  FitAddon: class {
    fit() {}
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
    this.readyState = 3
  }

  emit(type: string, event: unknown) {
    for (const listener of this.listeners.get(type) ?? []) listener(event)
  }
}

beforeEach(() => {
  mocks.disposed = 0
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

/** The pane defers its socket by one tick; wait for it to exist. */
async function socketCount(expected: number): Promise<void> {
  await waitFor(() => expect(FakeSocket.instances).toHaveLength(expected))
}

describe('XtermPane', () => {
  it('keeps its socket when the parent passes a new callback', async () => {
    const { rerender } = render(
      <XtermPane url="ws://example.test/agents/t1/ws" onClosedWithoutOpen={() => undefined} />,
    )
    await socketCount(1)

    // A parent that re-renders (typing in its prompt box does this) must not
    // tear the terminal down; a reconnect is a visible full repaint.
    rerender(
      <XtermPane url="ws://example.test/agents/t1/ws" onClosedWithoutOpen={() => undefined} />,
    )
    await new Promise((resolve) => setTimeout(resolve, 10))

    expect(FakeSocket.instances).toHaveLength(1)
    expect(mocks.disposed).toBe(0)
  })

  it('reconnects when the socket url changes', async () => {
    const { rerender } = render(<XtermPane url="ws://example.test/agents/t1/ws" />)
    await socketCount(1)
    const first = FakeSocket.instances[0]
    expect(first).toBeDefined()

    rerender(<XtermPane url="ws://example.test/agents/t2/ws" />)
    await waitFor(() => expect(FakeSocket.instances).toHaveLength(2))

    expect(first?.readyState).toBe(3)
  })
})
