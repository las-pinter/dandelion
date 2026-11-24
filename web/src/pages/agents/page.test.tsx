import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import type { AgentsState } from '@/hooks/use-agents'
import type { AgentAction, AgentInfo, AgentsSnapshot, ClaudeAgent } from '@/lib/agents'
import { AgentsPage, page } from './page'

const { useAgents } = vi.hoisted(() => ({
  useAgents: vi.fn<() => AgentsState>(),
}))

vi.mock('@/hooks/use-agents', () => ({ useAgents }))
vi.mock('@/components/XtermPane/XtermPane', () => ({
  XtermPane: ({ url }: { url: string }) => <div data-testid="xterm-pane">{url}</div>,
}))

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

const claude: ClaudeAgent = {
  terminal: 't7',
  sessionId: '0f3c2b1a-aaaa-bbbb-cccc-0123456789ab',
  cwd: '/home/dev/dandelion',
  name: 'dandelion-a6',
  agent: null,
  version: '2.1.330',
  status: 'waiting',
  waitingFor: 'permission prompt',
  updatedAt: 1795372085544,
  usage: {
    sessionId: '0f3c2b1a-aaaa-bbbb-cccc-0123456789ab',
    models: [],
    tokens: {
      inputTokens: 0,
      outputTokens: 0,
      cacheReadInputTokens: 0,
      cacheCreationInputTokens: 0,
    },
    costUsd: 0,
    unpricedModels: [],
    messages: 0,
  },
}

function state(overrides: Partial<AgentsState> = {}): AgentsState {
  return {
    snapshot,
    loading: false,
    error: null,
    busy: null,
    refresh: () => undefined,
    act: async () => undefined,
    spawn: async () => null,
    ...overrides,
  }
}

describe('AgentsPage', () => {
  it('lists the agents and opens the first one', () => {
    useAgents.mockReturnValue(state())

    render(<AgentsPage />)

    expect(screen.getByRole('heading', { name: 'Agents' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'claude in dandelion' })).toBeInTheDocument()
    expect(screen.getByTestId('xterm-pane')).toHaveTextContent(
      'ws://localhost:3000/api/agents/t7/ws',
    )
  })

  it('matches the selected agent to its Claude session', () => {
    useAgents.mockReturnValue(state({ snapshot: { ...snapshot, claude: [claude] } }))

    render(<AgentsPage />)

    expect(screen.getByText('Claude session')).toBeInTheDocument()
    expect(screen.getByText('0f3c2b1a')).toBeInTheDocument()
  })

  it('lists a Claude session outside herdr as unmanaged', () => {
    useAgents.mockReturnValue(
      state({
        snapshot: {
          ...snapshot,
          claude: [{ ...claude, terminal: null, name: 'plain-shell-session', cwd: '/tmp/plain' }],
        },
      }),
    )

    render(<AgentsPage />)

    expect(screen.getByText('unmanaged')).toBeInTheDocument()
    expect(screen.getByText('plain-shell-session')).toBeInTheDocument()
  })

  it('runs an action on the selected agent', async () => {
    const user = userEvent.setup()
    const act = vi.fn<(terminal: string, action: AgentAction) => Promise<void>>()
    useAgents.mockReturnValue(state({ act }))

    render(<AgentsPage />)
    await user.click(screen.getByRole('button', { name: 'Approve' }))

    expect(act).toHaveBeenCalledWith('t7', { kind: 'approve' })
  })

  it('explains a missing backend', () => {
    useAgents.mockReturnValue(
      state({
        snapshot: {
          available: false,
          backend: {
            kind: 'herdr_core',
            available: false,
            version: null,
            protocol: null,
            access: 'read_only',
            detail: 'no socket',
          },
          agents: [],
          claude: [],
        },
      }),
    )

    render(<AgentsPage />)

    expect(screen.getByText(/No terminal backend is reachable: no socket/)).toBeInTheDocument()
    expect(screen.getByText('No agent is running.')).toBeInTheDocument()
  })

  it('starts an agent in a directory', async () => {
    const user = userEvent.setup()
    const spawn = vi.fn<AgentsState['spawn']>(async () => agent)
    useAgents.mockReturnValue(state({ spawn }))

    render(<AgentsPage />)
    await user.clear(screen.getByLabelText('Command line'))
    await user.type(screen.getByLabelText('Command line'), 'claude')
    await user.type(screen.getByLabelText('Directory'), '/home/dev/worktree')
    await user.click(screen.getByRole('button', { name: 'Start' }))

    expect(spawn).toHaveBeenCalledWith({
      line: 'claude',
      cwd: '/home/dev/worktree',
      name: undefined,
    })
  })

  it('shows the error and registers its page', () => {
    useAgents.mockReturnValue(state({ error: 'no backend' }))

    render(<AgentsPage />)

    expect(screen.getByRole('alert')).toHaveTextContent('no backend')
    expect(page.path).toBe('/agents')
    expect(page.title).toBe('Agents')
    expect(page.order).toBe(1)
  })
})
