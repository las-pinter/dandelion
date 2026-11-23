import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import type { JournalEntry, UnitInfo } from '@/lib/services'
import { JournalPanel } from './JournalPanel'

const unit: UnitInfo = {
  name: 'nginx.service',
  description: 'A high performance web server',
  load_state: 'loaded',
  active_state: 'active',
  sub_state: 'running',
  scope: 'system',
}

const entries: JournalEntry[] = [
  {
    timestamp_usec: 1_700_000_000_000_000,
    message: 'started',
    priority: 6,
    unit: 'nginx.service',
    identifier: 'nginx',
    pid: 42,
  },
  {
    timestamp_usec: null,
    message: 'reloaded',
    priority: 3,
    unit: 'nginx.service',
    identifier: null,
    pid: null,
  },
]

describe('JournalPanel', () => {
  it('asks for a selection before a unit is chosen', () => {
    render(<JournalPanel unit={null} entries={[]} onRefresh={vi.fn<() => void>()} />)

    expect(screen.getByText('Select a unit above to read its journal.')).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Refresh' })).not.toBeInTheDocument()
  })

  it('renders the entries with priority and writer', () => {
    render(<JournalPanel unit={unit} entries={entries} onRefresh={vi.fn<() => void>()} />)

    expect(screen.getByText('nginx.service (system)')).toBeInTheDocument()
    expect(screen.getByText('loaded')).toBeInTheDocument()
    expect(screen.getByText(/active \(running\)/)).toBeInTheDocument()
    expect(screen.getByText('started')).toBeInTheDocument()
    expect(screen.getByText('reloaded')).toBeInTheDocument()
    expect(screen.getByText('info')).toBeInTheDocument()
    expect(screen.getByText('err')).toBeInTheDocument()
    expect(screen.getByText('nginx')).toBeInTheDocument()
    expect(screen.getByText('·')).toBeInTheDocument()
  })

  it('reports an empty journal and an error', () => {
    const { rerender } = render(
      <JournalPanel unit={unit} entries={[]} onRefresh={vi.fn<() => void>()} />,
    )
    expect(screen.getByText('No journal entries.')).toBeInTheDocument()

    rerender(
      <JournalPanel
        unit={unit}
        entries={[]}
        error="journal unavailable"
        onRefresh={vi.fn<() => void>()}
      />,
    )
    expect(screen.getByRole('alert')).toHaveTextContent('journal unavailable')
  })

  it('refreshes on demand', async () => {
    const user = userEvent.setup()
    const onRefresh = vi.fn<() => void>()
    render(<JournalPanel unit={unit} entries={entries} onRefresh={onRefresh} />)

    await user.click(screen.getByRole('button', { name: 'Refresh' }))

    expect(onRefresh).toHaveBeenCalledTimes(1)
  })

  it('shows a loading line', () => {
    render(<JournalPanel unit={unit} entries={[]} loading onRefresh={vi.fn<() => void>()} />)

    expect(screen.getByText('Reading the journal…')).toBeInTheDocument()
  })
})
