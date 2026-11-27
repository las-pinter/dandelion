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

  it('enlarges the transcript over the chat until a prompt or the toggle returns the split', async () => {
    const user = userEvent.setup()
    setup()

    const strip = () => screen.getByTestId('xterm-pane').closest('.overflow-hidden')
    await user.click(screen.getByRole('button', { name: 'Enlarge the transcript' }))
    expect(screen.getByRole('button', { name: 'Enlarge the transcript' })).toHaveAttribute(
      'aria-pressed',
      'true',
    )
    expect(strip()).toHaveClass('hidden')

    // Sending a prompt brings the turn back into view.
    await user.type(screen.getByLabelText('Prompt'), 'go{Enter}')

    expect(strip()).not.toHaveClass('hidden')
    expect(screen.getByRole('button', { name: 'Enlarge the transcript' })).toHaveAttribute(
      'aria-pressed',
      'false',
    )
  })

  it('returns the split when a closed terminal is shown again', async () => {
    const user = userEvent.setup()
    setup({ agent: withState({ state: 'idle', blocked_reason: null, progress: null }) })

    await user.click(screen.getByRole('button', { name: 'Enlarge the transcript' }))
    await user.click(screen.getByRole('button', { name: 'Show terminal' }))

    // The new terminal must be visible, not hidden behind the enlarged
    // transcript.
    expect(screen.getByTestId('xterm-pane').closest('.overflow-hidden')).not.toHaveClass('hidden')
    expect(screen.getByRole('log', { name: 'Transcript' }).closest('.hidden')).toBeNull()
  })

  it('shows the split when the agent blocks, so the dialog is not hidden', async () => {
    const user = userEvent.setup()
    const onAct = vi.fn<(action: AgentAction) => void>()
    const question = withState({ state: 'blocked', blocked_reason: 'question', progress: null })
    const { rerender } = render(<AgentChat agent={agent} claude={claude} onAct={onAct} />)

    await user.click(screen.getByRole('button', { name: 'Enlarge the transcript' }))
    rerender(<AgentChat agent={question} claude={claude} onAct={onAct} />)

    expect(screen.getByRole('log', { name: 'Transcript' }).closest('.hidden')).toBeNull()
    expect(screen.getByRole('form', { name: 'Answer the question' })).toBeInTheDocument()
  })

  it('enlarges the live output and gives the chat back when the agent settles', async () => {
    const user = userEvent.setup()
    const onAct = vi.fn<(action: AgentAction) => void>()
    const idle = withState({ state: 'idle', blocked_reason: null, progress: null })
    const { rerender } = render(<AgentChat agent={idle} claude={claude} onAct={onAct} />)
    await user.click(screen.getByRole('button', { name: 'Show terminal' }))
    await user.click(screen.getByRole('button', { name: 'Enlarge the live output' }))

    const transcript = () => screen.getByRole('log', { name: 'Transcript' })
    expect(transcript().closest('.hidden')).not.toBeNull()
    expect(screen.getByTestId('xterm-pane').closest('.overflow-hidden')).toHaveClass('flex-1')

    // The settled agent closes the strip; an enlarged terminal must not leave
    // the chat with nothing in it.
    rerender(<AgentChat agent={agent} claude={claude} onAct={onAct} />)
    rerender(<AgentChat agent={idle} claude={claude} onAct={onAct} />)

    expect(screen.queryByTestId('xterm-pane')).toBeNull()
    expect(transcript().closest('.hidden')).toBeNull()
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

  it('focuses the swapped field without scrolling the page', () => {
    // On a phone a plain focus() drags the page down to the composer.
    const focus = vi.spyOn(HTMLTextAreaElement.prototype, 'focus')
    try {
      const onAct = vi.fn<(action: AgentAction) => void>()
      const question = withState({ state: 'blocked', blocked_reason: 'question', progress: null })
      const { rerender } = render(<AgentChat agent={agent} claude={claude} onAct={onAct} />)

      rerender(<AgentChat agent={question} claude={claude} onAct={onAct} />)
      expect(focus.mock.lastCall).toEqual([{ preventScroll: true }])

      rerender(<AgentChat agent={agent} claude={claude} onAct={onAct} />)
      expect(focus.mock.lastCall).toEqual([{ preventScroll: true }])
    } finally {
      focus.mockRestore()
    }
  })

  it('scrolls to the field when it really is off screen', () => {
    // A viewport too short for the chat: the scroll is the point there.
    const focus = vi.spyOn(HTMLTextAreaElement.prototype, 'focus')
    const box = vi
      .spyOn(HTMLTextAreaElement.prototype, 'getBoundingClientRect')
      .mockReturnValue(new DOMRect(0, window.innerHeight + 200, 300, 40))
    try {
      const onAct = vi.fn<(action: AgentAction) => void>()
      const question = withState({ state: 'blocked', blocked_reason: 'question', progress: null })
      const { rerender } = render(<AgentChat agent={agent} claude={claude} onAct={onAct} />)

      rerender(<AgentChat agent={question} claude={claude} onAct={onAct} />)

      expect(focus.mock.lastCall).toEqual([{ preventScroll: false }])
    } finally {
      focus.mockRestore()
      box.mockRestore()
    }
  })

  it('counts the top bar as covering the field', () => {
    // A field scrolled up under the sticky bar is hidden too.
    const bar = document.createElement('div')
    bar.dataset.appChrome = 'top'
    document.body.append(bar)
    const focus = vi.spyOn(HTMLTextAreaElement.prototype, 'focus')
    const box = vi
      .spyOn(HTMLTextAreaElement.prototype, 'getBoundingClientRect')
      .mockReturnValue(new DOMRect(0, 0, 390, 40))
    const barBox = vi
      .spyOn(bar, 'getBoundingClientRect')
      .mockReturnValue(new DOMRect(0, 0, 390, 56))
    try {
      const onAct = vi.fn<(action: AgentAction) => void>()
      const question = withState({ state: 'blocked', blocked_reason: 'question', progress: null })
      const { rerender } = render(<AgentChat agent={agent} claude={claude} onAct={onAct} />)

      rerender(<AgentChat agent={question} claude={claude} onAct={onAct} />)

      expect(focus.mock.lastCall).toEqual([{ preventScroll: false }])
    } finally {
      focus.mockRestore()
      box.mockRestore()
      barBox.mockRestore()
      bar.remove()
    }
  })

  it('counts the tab bar as covering the field', () => {
    // A field just above the viewport bottom is still hidden by the bar.
    const bar = document.createElement('div')
    bar.dataset.appChrome = 'tab'
    document.body.append(bar)
    const focus = vi.spyOn(HTMLTextAreaElement.prototype, 'focus')
    const box = vi
      .spyOn(HTMLTextAreaElement.prototype, 'getBoundingClientRect')
      .mockReturnValue(new DOMRect(0, window.innerHeight - 68, 390, 40))
    const barBox = vi
      .spyOn(bar, 'getBoundingClientRect')
      .mockReturnValue(new DOMRect(0, window.innerHeight - 49, 390, 49))
    try {
      const onAct = vi.fn<(action: AgentAction) => void>()
      const question = withState({ state: 'blocked', blocked_reason: 'question', progress: null })
      const { rerender } = render(<AgentChat agent={agent} claude={claude} onAct={onAct} />)

      rerender(<AgentChat agent={question} claude={claude} onAct={onAct} />)

      expect(focus.mock.lastCall).toEqual([{ preventScroll: false }])
    } finally {
      focus.mockRestore()
      box.mockRestore()
      barBox.mockRestore()
      bar.remove()
    }
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
