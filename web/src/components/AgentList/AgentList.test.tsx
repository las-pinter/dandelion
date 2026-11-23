import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import type { AgentInfo, ClaudeAgent } from '@/lib/agents'
import { AgentList } from './AgentList'

const blocked: AgentInfo = {
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

const working: AgentInfo = {
  ...blocked,
  id: 't8',
  title: 'codex in worktree',
  cwd: '/home/dev/worktree',
  agent_kind: 'codex',
  agent_state: { state: 'working', blocked_reason: null, progress: null },
}

const unmanaged: ClaudeAgent = {
  terminal: null,
  sessionId: 'abcdef12-0000-0000-0000-000000000000',
  cwd: '/tmp/plain-shell',
  name: null,
  agent: null,
  version: '2.1.330',
  status: 'idle',
  waitingFor: null,
  updatedAt: null,
  usage: {
    sessionId: 'abcdef12-0000-0000-0000-000000000000',
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

function setup(
  agents = [blocked, working],
  selectedId: string | null = null,
  sessions: ClaudeAgent[] = [],
) {
  const onSelect = vi.fn<(id: string) => void>()
  render(
    <AgentList agents={agents} unmanaged={sessions} selectedId={selectedId} onSelect={onSelect} />,
  )
  return { onSelect }
}

describe('AgentList', () => {
  it('shows every agent with its state and directory', () => {
    setup()

    expect(screen.getByRole('button', { name: 'claude in dandelion' })).toBeInTheDocument()
    expect(screen.getByText('needs permission')).toBeInTheDocument()
    expect(screen.getByText('/home/dev/worktree')).toBeInTheDocument()
    expect(screen.getByText('codex · t8')).toBeInTheDocument()
  })

  it('marks the selected agent and reports a click', async () => {
    const user = userEvent.setup()
    const { onSelect } = setup([blocked, working], 't7')

    expect(screen.getByRole('button', { name: 'claude in dandelion' })).toHaveAttribute(
      'aria-pressed',
      'true',
    )
    await user.click(screen.getByRole('button', { name: 'codex in worktree' }))
    expect(onSelect).toHaveBeenCalledWith('t8')
  })

  it('lists a session outside herdr as unmanaged', () => {
    setup([blocked], null, [unmanaged])

    expect(screen.getByText('unmanaged')).toBeInTheDocument()
    expect(screen.getByText('session abcdef12')).toBeInTheDocument()
    expect(screen.getByText('/tmp/plain-shell')).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'session abcdef12' })).toBeNull()
  })

  it('reports an empty list', () => {
    setup([])

    expect(screen.getByText('No agent is running.')).toBeInTheDocument()
  })
})
