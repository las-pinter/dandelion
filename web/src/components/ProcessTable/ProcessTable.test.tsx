import { render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import type { ProcessInfo, ProcessSignal } from '@/lib/processes'
import { ProcessTable } from './ProcessTable'

const processes: ProcessInfo[] = [
  {
    pid: 1,
    parent_pid: null,
    name: 'systemd',
    user: 'root',
    state: 'sleeping',
    cpu_seconds: 1.5,
    memory_bytes: 2 * 1024 ** 2,
    command: '/sbin/init',
    started_at_unix: 1_700_000_000,
    niceness: 0,
  },
  {
    pid: 812,
    parent_pid: 1,
    name: 'node',
    user: 'dev',
    state: 'running',
    cpu_seconds: 75,
    memory_bytes: 3 * 1024 ** 3,
    command: 'node vite',
    started_at_unix: 1_700_000_500,
    niceness: 0,
  },
  {
    pid: 1440,
    parent_pid: 812,
    name: 'postgres',
    user: 'postgres',
    state: 'disk_sleep',
    cpu_seconds: 3_725,
    memory_bytes: 512 * 1024 ** 2,
    command: 'postgres -D /var/lib/postgresql/data',
    started_at_unix: 1_700_001_200,
    niceness: 5,
  },
]

function setup(overrides: Partial<Parameters<typeof ProcessTable>[0]> = {}) {
  const onSignal = vi.fn<(pid: number, signal: ProcessSignal) => void>()
  const onRenice = vi.fn<(pid: number, priority: number) => void>()
  render(
    <ProcessTable processes={processes} onSignal={onSignal} onRenice={onRenice} {...overrides} />,
  )
  return { onSignal, onRenice }
}

describe('ProcessTable', () => {
  it('renders pid, state, CPU time, memory and nice value', () => {
    setup()

    const row = screen.getByRole('row', { name: /node/ })
    expect(within(row).getByText('812')).toBeInTheDocument()
    expect(within(row).getByText('Running')).toBeInTheDocument()
    expect(within(row).getByText('1m 15s')).toBeInTheDocument()
    expect(within(row).getByText('3.0 GiB')).toBeInTheDocument()

    const slow = screen.getByRole('row', { name: /postgres/ })
    expect(within(slow).getByText('Disk sleep')).toBeInTheDocument()
    expect(within(slow).getByText('1h 2m')).toBeInTheDocument()
    expect(within(slow).getByText('5')).toBeInTheDocument()
  })

  it('filters by pid, name, command and user', async () => {
    const user = userEvent.setup()
    setup()

    await user.type(screen.getByLabelText('Search processes'), 'postgres')

    expect(screen.getByRole('row', { name: /postgres/ })).toBeInTheDocument()
    expect(screen.queryByRole('row', { name: /node/ })).not.toBeInTheDocument()
    expect(screen.getByText('1 of 3 processes')).toBeInTheDocument()
  })

  it('reports when nothing matches', async () => {
    const user = userEvent.setup()
    setup()

    await user.type(screen.getByLabelText('Search processes'), 'nope')

    expect(screen.getByText('No processes match “nope”.')).toBeInTheDocument()
  })

  it('disables actions for pid 0 and pid 1', () => {
    setup()

    expect(screen.getByRole('button', { name: 'Terminate systemd (1)' })).toBeDisabled()
    expect(screen.getByRole('button', { name: 'Force kill systemd (1)' })).toBeDisabled()
    expect(screen.getByRole('button', { name: 'Renice systemd (1)' })).toBeDisabled()
    expect(screen.getByRole('button', { name: 'Terminate node (812)' })).toBeEnabled()
  })

  it('confirms SIGTERM before signalling', async () => {
    const user = userEvent.setup()
    const { onSignal } = setup()

    await user.click(screen.getByRole('button', { name: 'Terminate node (812)' }))
    expect(
      screen.getByText(/Send SIGTERM so the process can shut down cleanly/),
    ).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Terminate' }))

    expect(onSignal).toHaveBeenCalledWith(812, 'term')
  })

  it('confirms SIGKILL before signalling', async () => {
    const user = userEvent.setup()
    const { onSignal } = setup()

    await user.click(screen.getByRole('button', { name: 'Force kill node (812)' }))
    await user.click(screen.getByRole('button', { name: 'Force kill' }))

    expect(onSignal).toHaveBeenCalledWith(812, 'kill')
  })

  it('renices with the entered value', async () => {
    const user = userEvent.setup()
    const { onRenice } = setup()

    await user.click(screen.getByRole('button', { name: 'Renice postgres (1440)' }))
    const input = screen.getByLabelText('Nice value (-20 to 19)')
    expect(input).toHaveValue(5)
    await user.clear(input)
    await user.type(input, '10')
    await user.click(screen.getByRole('button', { name: 'Save' }))

    expect(onRenice).toHaveBeenCalledWith(1440, 10)
  })

  it('refuses a nice value outside the kernel range', async () => {
    const user = userEvent.setup()
    const { onRenice } = setup()

    await user.click(screen.getByRole('button', { name: 'Renice node (812)' }))
    const input = screen.getByLabelText('Nice value (-20 to 19)')
    await user.clear(input)
    await user.type(input, '50')

    expect(screen.getByText('Enter a whole number from -20 to 19.')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled()
    expect(onRenice).not.toHaveBeenCalled()
  })

  it('announces an action error and a busy row', () => {
    setup({ error: 'permission denied for process 812', busyPid: 812 })

    expect(screen.getByRole('alert')).toHaveTextContent('permission denied for process 812')
    expect(screen.getByRole('button', { name: 'Terminate node (812)' })).toBeDisabled()
  })

  it('shows a loading line before the first page arrives', () => {
    setup({ processes: [], loading: true })

    expect(screen.getByText('Reading processes…')).toBeInTheDocument()
  })
})
