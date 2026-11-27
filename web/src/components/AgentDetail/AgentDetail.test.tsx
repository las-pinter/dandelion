import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import type { AgentAction, AgentInfo, ClaudeAgent } from '@/lib/agents'
import { AgentDetail } from './AgentDetail'

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

vi.mock('@/components/XtermPane/XtermPane', () => ({
  XtermPane: ({
    url,
    readOnly,
    onClosedWithoutOpen,
  }: {
    url: string
    readOnly?: boolean
    onClosedWithoutOpen?: (reason?: string) => void
  }) => (
    <div data-testid="xterm-pane" data-readonly={String(readOnly)}>
      {url}
      <button type="button" onClick={() => onClosedWithoutOpen?.('cannot attach to this agent')}>
        fail the pane
      </button>
    </div>
  ),
}))

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
    models: [
      {
        model: 'claude-opus-5-5',
        tokens: {
          inputTokens: 0,
          outputTokens: 0,
          cacheReadInputTokens: 0,
          cacheCreationInputTokens: 0,
        },
        costUsd: 0.42,
      },
    ],
    tokens: {
      inputTokens: 96,
      outputTokens: 39_430,
      cacheReadInputTokens: 3_233_619,
      cacheCreationInputTokens: 129_107,
    },
    costUsd: 0.42,
    unpricedModels: [],
    messages: 41,
  },
}

function setup(agent: AgentInfo = blocked, session: ClaudeAgent | null = null) {
  const onAct = vi.fn<(action: AgentAction) => void>()
  render(<AgentDetail agent={agent} claude={session} onAct={onAct} />)
  return { onAct }
}

describe('AgentDetail', () => {
  it('shows a read-only live pane and the blocked prompt', () => {
    setup()

    const pane = screen.getByTestId('xterm-pane')
    expect(pane).toHaveTextContent('ws://localhost:3000/api/agents/t7/ws')
    expect(pane).toHaveAttribute('data-readonly', 'true')
    expect(screen.getByText(/needs permission/)).toBeInTheDocument()
  })

  it('shows the Claude session, model, tokens and cost', () => {
    setup(blocked, claude)

    expect(screen.getByText('Claude session')).toBeInTheDocument()
    expect(screen.getByText('0f3c2b1a')).toBeInTheDocument()
    expect(screen.getByText('claude-opus-5-5')).toBeInTheDocument()
    expect(screen.getByText('3.4M')).toBeInTheDocument()
    expect(screen.getByText('~$0.42')).toBeInTheDocument()
    expect(screen.getByText('a permission prompt')).toBeInTheDocument()
  })

  it('expands and collapses the view without dropping it', async () => {
    const user = userEvent.setup()
    render(<AgentDetail agent={blocked} claude={claude} onAct={() => undefined} />)
    const pane = screen.getByTestId('xterm-pane')

    await user.click(screen.getByRole('button', { name: 'Full screen' }))

    const exit = screen.getByRole('button', { name: 'Exit full screen' })
    expect(screen.getByRole('dialog', { name: 'Expanded agent view' })).toBeInTheDocument()
    expect(screen.getByTestId('xterm-pane')).toBe(pane)

    await user.click(exit)
    expect(screen.getByTestId('xterm-pane')).toBe(pane)
    expect(screen.queryByRole('dialog')).toBeNull()
  })

  it('keeps keyboard focus inside the expanded view, then returns it', async () => {
    const user = userEvent.setup()
    render(<AgentDetail agent={blocked} claude={claude} onAct={() => undefined} />)

    await user.click(screen.getByRole('button', { name: 'Full screen' }))
    const dialog = screen.getByRole('dialog', { name: 'Expanded agent view' })
    expect(dialog).toHaveFocus()

    // The trigger is behind the modal now, so tabbing must never leave it.
    for (let press = 0; press < 40; press += 1) {
      await user.tab()
      expect(dialog.contains(document.activeElement)).toBe(true)
    }
    await user.tab({ shift: true })
    expect(dialog.contains(document.activeElement)).toBe(true)

    await user.keyboard('{Escape}')
    expect(screen.getByRole('button', { name: 'Full screen' })).toHaveFocus()
  })

  it('closes the expanded view on Escape', async () => {
    const user = userEvent.setup()
    render(<AgentDetail agent={blocked} claude={claude} onAct={() => undefined} />)

    await user.click(screen.getByRole('button', { name: 'Full screen' }))
    await user.keyboard('{Escape}')

    expect(screen.queryByRole('dialog')).toBeNull()
    expect(screen.getByRole('button', { name: 'Full screen' })).toBeInTheDocument()
  })

  it('says why the pane could not attach', async () => {
    const user = userEvent.setup()
    setup()

    await user.click(screen.getByRole('button', { name: 'fail the pane' }))

    expect(screen.getByRole('alert')).toHaveTextContent('cannot attach to this agent')
  })

  it('approves and denies a waiting prompt', async () => {
    const user = userEvent.setup()
    const { onAct } = setup()

    await user.click(screen.getByRole('button', { name: 'Approve' }))
    await user.click(screen.getByRole('button', { name: 'Deny' }))

    expect(onAct).toHaveBeenNthCalledWith(1, { kind: 'approve' })
    expect(onAct).toHaveBeenNthCalledWith(2, { kind: 'deny' })
  })

  it('disables approve while nothing waits', () => {
    setup({ ...blocked, agent_state: { state: 'idle', blocked_reason: null, progress: null } })

    expect(screen.getByRole('button', { name: 'Approve' })).toBeDisabled()
    expect(screen.getByRole('button', { name: 'Deny' })).toBeDisabled()
  })

  it('sends a typed prompt and clears the box', async () => {
    const user = userEvent.setup()
    const { onAct } = setup()

    await user.type(screen.getByLabelText('Prompt'), 'run the tests')
    await user.click(screen.getByRole('button', { name: 'Send' }))

    expect(onAct).toHaveBeenCalledWith({ kind: 'prompt', text: 'run the tests' })
    expect(screen.getByLabelText('Prompt')).toHaveValue('')
  })

  it('interrupts and confirms a kill', async () => {
    const user = userEvent.setup()
    const { onAct } = setup()

    await user.click(screen.getByRole('button', { name: 'Interrupt' }))
    expect(onAct).toHaveBeenCalledWith({ kind: 'interrupt' })

    await user.click(screen.getByRole('button', { name: 'Kill' }))
    expect(screen.getByText('Kill claude in dandelion?')).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Kill' }))
    expect(onAct).toHaveBeenCalledWith({ kind: 'kill' })
  })

  it('marks the action in flight', () => {
    const onAct = vi.fn<(action: AgentAction) => void>()
    render(<AgentDetail agent={blocked} busy="t7:approve" onAct={onAct} />)

    expect(screen.getByRole('button', { name: 'Approve' })).toBeDisabled()
  })
})

describe('AgentDetail chat surface', () => {
  it('shows the transcript and the live strip together', () => {
    render(<AgentDetail agent={blocked} claude={claude} onAct={() => undefined} />)

    expect(screen.getByRole('log', { name: 'Transcript' })).toBeInTheDocument()
    expect(screen.getByTestId('xterm-pane')).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Terminal' })).toBeNull()
    expect(screen.queryByRole('button', { name: 'Transcript' })).toBeNull()
  })

  it('swaps the prompt for the answer bar on a question', () => {
    render(
      <AgentDetail
        agent={{
          ...blocked,
          agent_state: { state: 'blocked', blocked_reason: 'question', progress: null },
        }}
        onAct={() => undefined}
      />,
    )

    expect(screen.getByRole('form', { name: 'Answer the question' })).toBeInTheDocument()
    expect(screen.queryByLabelText('Prompt')).toBeNull()
    expect(screen.queryByRole('button', { name: 'Approve' })).toBeNull()
    expect(screen.getByRole('button', { name: 'Interrupt' })).toBeInTheDocument()
  })
})
