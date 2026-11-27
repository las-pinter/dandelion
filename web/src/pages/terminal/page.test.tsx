import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import type { TerminalsState } from '@/hooks/use-terminals'
import type { TerminalInfo } from '@/lib/terminal'
import { TerminalPage } from './screen'
import { page } from './page'

const { useTerminals } = vi.hoisted(() => ({
  useTerminals: vi.fn<() => TerminalsState>(),
}))

vi.mock('@/hooks/use-terminals', () => ({ useTerminals }))
vi.mock('@/components/TerminalTabs/TerminalTabs', () => ({
  TerminalTabs: ({ terminals }: { terminals: TerminalInfo[] }) => (
    <div data-testid="terminal-tabs">{terminals.length} tabs</div>
  ),
}))

const terminal: TerminalInfo = {
  id: 'aaa111',
  shell: '/bin/bash',
  cols: 80,
  rows: 24,
  created_at_unix: 1_700_000_000,
  attached: false,
}

function state(overrides: Partial<TerminalsState> = {}): TerminalsState {
  return {
    error: null,
    busy: false,
    open: async () => null,
    close: async () => undefined,
    ...overrides,
  }
}

describe('TerminalPage', () => {
  it('opens a shell from the empty state and shows its tab', async () => {
    const user = userEvent.setup()
    const open = vi.fn<TerminalsState['open']>(async () => terminal)
    useTerminals.mockReturnValue(state({ open }))

    render(<TerminalPage />)

    expect(screen.getByRole('heading', { name: 'Terminal' })).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Open a shell' }))

    expect(open).toHaveBeenCalled()
    expect(await screen.findByTestId('terminal-tabs')).toHaveTextContent('1 tabs')
  })

  it('shows the error and disables opening while busy', () => {
    useTerminals.mockReturnValue(state({ error: 'the shell could not be opened', busy: true }))

    render(<TerminalPage />)

    expect(screen.getByRole('alert')).toHaveTextContent('the shell could not be opened')
    expect(screen.getByRole('button', { name: 'New tab' })).toBeDisabled()
  })

  it('stops at the shell limit', async () => {
    const user = userEvent.setup()
    useTerminals.mockReturnValue(state({ open: async () => terminal }))

    render(<TerminalPage />)
    const newTab = screen.getByRole('button', { name: 'New tab' })
    for (let index = 0; index < 8; index += 1) {
      await user.click(newTab)
    }

    expect(await screen.findByText(/At most 8 shells/)).toBeInTheDocument()
    expect(newTab).toBeDisabled()
  })

  it('registers its route and navigation entry', () => {
    expect(page.path).toBe('/terminal')
    expect(page.title).toBe('Terminal')
    expect(page.order).toBe(5)
  })
})
