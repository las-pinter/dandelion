import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { AgentAction, AgentInfo, ClaudeAgent } from '@/lib/agents'
import { AgentChat } from './AgentChat'

vi.mock('@/hooks/use-transcript', () => ({
  useTranscript: () => ({
    records: [],
    subagents: [],
    hasEarlier: false,
    loading: false,
    error: null,
    loadEarlier: () => undefined,
  }),
}))

const xterm = vi.hoisted(() => ({ mounts: 0 }))

vi.mock('@/components/XtermPane/XtermPane', () => ({
  XtermPane: ({
    url,
    readOnly,
    onClosedWithoutOpen,
  }: {
    url: string
    readOnly?: boolean
    onClosedWithoutOpen?: (reason?: string) => void
  }) => {
    xterm.mounts += 1
    return (
      <div data-testid="xterm-pane" data-readonly={String(readOnly)}>
        {url}
        <button type="button" onClick={() => onClosedWithoutOpen?.('cannot attach to this agent')}>
          fail the pane
        </button>
      </div>
    )
  },
}))

beforeEach(() => {
  xterm.mounts = 0
})

const agent: AgentInfo = {
  id: 't7',
  title: 'claude in dandelion',
  cwd: '/home/dev/dandelion',
  labels: {},
  agent_kind: 'claude-code',
  agent_state: { state: 'working', blocked_reason: null, progress: null },
  lifecycle: 'live',
  created_ms: 1_700_000_000_000,
  exit_code: null,
  finished_reason: null,
}

const claude: ClaudeAgent = {
  terminal: 't7',
  sessionId: '0f3c2b1a-aaaa-bbbb-cccc-0123456789ab',
  cwd: '/home/dev/dandelion',
  name: 'dandelion-a6',
  agent: null,
  version: '2.1.330',
  status: 'busy',
  waitingFor: null,
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

function withState(state: AgentInfo['agent_state']): AgentInfo {
  return { ...agent, agent_state: state }
}

function setup(overrides: Partial<Parameters<typeof AgentChat>[0]> = {}) {
  const onAct = vi.fn<(action: AgentAction) => void>()
  render(<AgentChat agent={agent} claude={claude} onAct={onAct} {...overrides} />)
  return { onAct }
}

describe('AgentChat', () => {
  it('shows the transcript above the live strip while the agent works', () => {
    setup()

    expect(screen.getByRole('log', { name: 'Transcript' })).toBeInTheDocument()
    expect(screen.getByTestId('xterm-pane')).toHaveTextContent(
      'ws://localhost:3000/api/agents/t7/ws',
    )
    expect(screen.getByTestId('xterm-pane')).toHaveAttribute('data-readonly', 'true')
  })

  it('collapses the strip when the agent settles, and the toggle restores it', async () => {
    const user = userEvent.setup()
    setup({ agent: withState({ state: 'idle', blocked_reason: null, progress: null }) })

    expect(screen.queryByTestId('xterm-pane')).toBeNull()
    await user.click(screen.getByRole('button', { name: 'Show terminal' }))
    expect(screen.getByTestId('xterm-pane')).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Hide terminal' }))
    expect(screen.queryByTestId('xterm-pane')).toBeNull()
  })

  it('opens the strip as soon as a prompt is sent', async () => {
    const user = userEvent.setup()
    const { onAct } = setup({
      agent: withState({ state: 'idle', blocked_reason: null, progress: null }),
    })

    await user.type(screen.getByLabelText('Prompt'), 'run the tests{Enter}')

    expect(onAct).toHaveBeenCalledWith({ kind: 'prompt', text: 'run the tests' })
    expect(screen.getByTestId('xterm-pane')).toBeInTheDocument()
  })

  it('collapses the strip once the turn it opened for settles', async () => {
    const user = userEvent.setup()
    const onAct = vi.fn<(action: AgentAction) => void>()
    const idle = withState({ state: 'idle', blocked_reason: null, progress: null })
    const { rerender } = render(<AgentChat agent={idle} claude={claude} onAct={onAct} />)

    await user.type(screen.getByLabelText('Prompt'), 'go{Enter}')
    expect(screen.getByTestId('xterm-pane')).toBeInTheDocument()

    rerender(<AgentChat agent={agent} claude={claude} onAct={onAct} />)
    expect(screen.getByTestId('xterm-pane')).toBeInTheDocument()

    rerender(<AgentChat agent={idle} claude={claude} onAct={onAct} />)
    expect(screen.queryByTestId('xterm-pane')).toBeNull()
  })

  it('opens a hidden strip when the agent blocks, so the dialog is visible', () => {
    const onAct = vi.fn<(action: AgentAction) => void>()
    const idle = withState({ state: 'idle', blocked_reason: null, progress: null })
    const blocked = withState({ state: 'blocked', blocked_reason: 'permission', progress: null })
    const { rerender } = render(<AgentChat agent={idle} claude={claude} onAct={onAct} />)
    expect(screen.queryByTestId('xterm-pane')).toBeNull()

    rerender(<AgentChat agent={blocked} claude={claude} onAct={onAct} />)
    expect(screen.getByTestId('xterm-pane')).toBeInTheDocument()
  })

  it('replaces a failed pane when the next turn starts', async () => {
    const user = userEvent.setup()
    setup()

    await user.click(screen.getByRole('button', { name: 'fail the pane' }))
    expect(screen.getByRole('alert')).toBeInTheDocument()
    const before = xterm.mounts

    await user.type(screen.getByLabelText('Prompt'), 'again{Enter}')

    expect(screen.queryByRole('alert')).toBeNull()
    expect(xterm.mounts).toBeGreaterThan(before)
  })

  it('moves focus between the prompt and the answer bar', () => {
    const onAct = vi.fn<(action: AgentAction) => void>()
    const question = withState({ state: 'blocked', blocked_reason: 'question', progress: null })
    const { rerender } = render(<AgentChat agent={agent} claude={claude} onAct={onAct} />)

    rerender(<AgentChat agent={question} claude={claude} onAct={onAct} />)
    expect(screen.getByLabelText('Answer')).toHaveFocus()

    rerender(<AgentChat agent={agent} claude={claude} onAct={onAct} />)
    expect(screen.getByLabelText('Prompt')).toHaveFocus()
  })

  it('starts a reopened strip without the last attach error', async () => {
    const user = userEvent.setup()
    setup({ agent: withState({ state: 'idle', blocked_reason: null, progress: null }) })

    await user.click(screen.getByRole('button', { name: 'Show terminal' }))
    await user.click(screen.getByRole('button', { name: 'fail the pane' }))
    expect(screen.getByRole('alert')).toHaveTextContent('cannot attach to this agent')

    await user.click(screen.getByRole('button', { name: 'Hide terminal' }))
    await user.click(screen.getByRole('button', { name: 'Show terminal' }))
    expect(screen.queryByRole('alert')).toBeNull()
  })

  it('sends with Enter and keeps Shift+Enter as a newline', async () => {
    const user = userEvent.setup()
    const { onAct } = setup()

    await user.type(screen.getByLabelText('Prompt'), 'first{Shift>}{Enter}{/Shift}second')
    expect(onAct).not.toHaveBeenCalled()
    expect(screen.getByLabelText('Prompt')).toHaveValue('first\nsecond')

    await user.keyboard('{Enter}')
    expect(onAct).toHaveBeenCalledWith({ kind: 'prompt', text: 'first\nsecond' })
    expect(screen.getByLabelText('Prompt')).toHaveValue('')
  })

  it('offers the answer bar instead of the prompt for a question', () => {
    setup({ agent: withState({ state: 'blocked', blocked_reason: 'question', progress: null }) })

    expect(screen.getByRole('form', { name: 'Answer the question' })).toBeInTheDocument()
    expect(screen.queryByLabelText('Prompt')).toBeNull()
  })

  it('always shows the strip for an agent without a transcript', () => {
    setup({
      agent: withState({ state: 'idle', blocked_reason: null, progress: null }),
      claude: null,
    })

    expect(screen.getByTestId('xterm-pane')).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Hide terminal' })).toBeNull()
  })
})
