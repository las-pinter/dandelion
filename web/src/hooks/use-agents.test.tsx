import { act, renderHook, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { AgentInfo, AgentsSnapshot } from '@/lib/agents'
import { useAgents } from './use-agents'

const { get, post } = vi.hoisted(() => ({
  get: vi.fn<(path: string, options?: unknown) => Promise<{ data?: unknown; error?: unknown }>>(),
  post: vi.fn<(path: string, options?: unknown) => Promise<{ data?: unknown; error?: unknown }>>(),
}))

vi.mock('@/api/client', () => ({ api: { GET: get, POST: post } }))

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
}

const agent: AgentInfo = {
  id: 't7',
  title: 'claude in dandelion',
  cwd: '/home/dev/dandelion',
  labels: {},
  agent_kind: 'claude-code',
  agent_state: { state: 'blocked', blocked_reason: 'permission', progress: null },
  lifecycle: 'live',
  created_ms: 1_700_000_000_000,
  exit_code: null,
  finished_reason: null,
}

const snapshot: AgentsSnapshot = {
  available: true,
  backend: {
    kind: 'herdr_core',
    available: true,
    version: '0.9.5',
    protocol: null,
    access: 'read_write',
    detail: null,
  },
  agents: [agent],
  claude: [],
}

beforeEach(() => {
  get.mockReset()
  post.mockReset()
  FakeSocket.instances = []
  vi.stubGlobal('WebSocket', FakeSocket)
})

describe('useAgents', () => {
  it('loads the snapshot and subscribes to the agents topic', async () => {
    get.mockResolvedValue({ data: snapshot })

    const { result } = renderHook(() => useAgents())

    expect(result.current.loading).toBe(true)
    await waitFor(() => expect(result.current.loading).toBe(false))
    expect(result.current.snapshot).toEqual(snapshot)
    expect(get).toHaveBeenCalledWith('/api/agents')

    const socket = FakeSocket.instances[0]
    expect(socket?.url).toBe('ws://localhost:3000/api/ws')
    socket?.emit('open', {})
    expect(socket?.sent).toContain('{"type":"subscribe","topics":["agents"]}')
  })

  it('refetches when a state event arrives', async () => {
    get.mockResolvedValue({ data: snapshot })
    const { result } = renderHook(() => useAgents())
    await waitFor(() => expect(result.current.loading).toBe(false))
    get.mockClear()

    act(() => {
      FakeSocket.instances[0]?.emit('message', {
        data: JSON.stringify({ type: 'event', topic: 'agents', data: { kind: 'terminal' } }),
      })
    })

    await waitFor(() => expect(get).toHaveBeenCalledTimes(1))
  })

  it('refetches after the hub reconnects', async () => {
    get.mockResolvedValue({ data: snapshot })
    const { result } = renderHook(() => useAgents())
    await waitFor(() => expect(result.current.loading).toBe(false))

    const first = FakeSocket.instances[0]
    first?.emit('open', {})
    get.mockClear()

    act(() => first?.emit('close', {}))
    await waitFor(() => expect(FakeSocket.instances.length).toBe(2), { timeout: 2_500 })
    act(() => FakeSocket.instances[1]?.emit('open', {}))

    await waitFor(() => expect(get).toHaveBeenCalledTimes(1))
  })

  it('reports a failed snapshot', async () => {
    get.mockResolvedValue({ error: { message: 'no backend' } })

    const { result } = renderHook(() => useAgents())

    await waitFor(() => expect(result.current.loading).toBe(false))
    expect(result.current.error).toBe('no backend')
  })

  it('sends an action and reloads', async () => {
    get.mockResolvedValue({ data: snapshot })
    post.mockResolvedValue({})

    const { result } = renderHook(() => useAgents())
    await waitFor(() => expect(result.current.loading).toBe(false))
    get.mockClear()

    await act(async () => {
      await result.current.act('t7', { kind: 'approve' })
    })

    expect(post).toHaveBeenCalledWith('/api/agents/action', {
      body: { terminal: 't7', action: { kind: 'approve' } },
    })
    await waitFor(() => expect(get).toHaveBeenCalledTimes(1))
    expect(result.current.busy).toBeNull()
  })

  it('shows a refused action and does not reload', async () => {
    get.mockResolvedValue({ data: snapshot })
    post.mockResolvedValue({ error: { message: 'the agent is waiting on a permission prompt' } })

    const { result } = renderHook(() => useAgents())
    await waitFor(() => expect(result.current.loading).toBe(false))
    get.mockClear()

    await act(async () => {
      await result.current.act('t7', { kind: 'prompt', text: 'hi' })
    })

    expect(result.current.error).toContain('waiting on a permission prompt')
    expect(get).not.toHaveBeenCalled()
  })

  it('spawns an agent and returns it', async () => {
    get.mockResolvedValue({ data: snapshot })
    post.mockResolvedValue({ data: agent })

    const { result } = renderHook(() => useAgents())
    await waitFor(() => expect(result.current.loading).toBe(false))

    let spawned: AgentInfo | null = null
    await act(async () => {
      spawned = await result.current.spawn({ line: 'claude', cwd: '/worktree' })
    })

    expect(post).toHaveBeenCalledWith('/api/agents/spawn', {
      body: { line: 'claude', cwd: '/worktree' },
    })
    expect(spawned).toEqual(agent)
  })
})
