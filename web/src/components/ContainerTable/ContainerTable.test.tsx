import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import type { ContainerAction, ContainerInfo } from '@/lib/containers'
import { ContainerTable } from './ContainerTable'

const containers: ContainerInfo[] = [
  {
    id: 'aaa111bbb222',
    name: 'web',
    image: 'nginx:1',
    command: 'nginx -g daemon off;',
    state: 'running',
    status: 'Up 2 minutes',
    created_at_unix: 1_700_000_000,
    labels: {},
  },
  {
    id: 'ccc333ddd444',
    name: 'db',
    image: 'postgres:17',
    command: null,
    state: 'exited',
    status: 'Exited (0) 3 hours ago',
    created_at_unix: 1_700_000_000,
    labels: {},
  },
]

function setup(overrides: Partial<Parameters<typeof ContainerTable>[0]> = {}) {
  const onSelect = vi.fn<(container: ContainerInfo) => void>()
  const onAction = vi.fn<(container: ContainerInfo, action: ContainerAction) => void>()
  render(
    <ContainerTable
      containers={containers}
      selected={null}
      onSelect={onSelect}
      onAction={onAction}
      {...overrides}
    />,
  )
  return { onSelect, onAction }
}

describe('ContainerTable', () => {
  it('renders the name, image and state', () => {
    setup()

    expect(screen.getByRole('button', { name: 'web' })).toBeInTheDocument()
    expect(screen.getByText('nginx:1')).toBeInTheDocument()
    expect(screen.getByText('Up 2 minutes')).toBeInTheDocument()
    expect(screen.getByText('2 of 2 containers')).toBeInTheDocument()
  })

  it('filters by text and by running state', async () => {
    const user = userEvent.setup()
    setup()

    await user.type(screen.getByLabelText('Search containers'), 'db')
    expect(screen.getByRole('button', { name: 'db' })).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'web' })).not.toBeInTheDocument()

    await user.clear(screen.getByLabelText('Search containers'))
    await user.selectOptions(screen.getByLabelText('Show'), 'running')
    expect(screen.getByRole('button', { name: 'web' })).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'db' })).not.toBeInTheDocument()

    await user.selectOptions(screen.getByLabelText('Show'), 'stopped')
    expect(screen.getByRole('button', { name: 'db' })).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'web' })).not.toBeInTheDocument()
  })

  it('gates each action on the container state', () => {
    setup()

    expect(screen.getByRole('button', { name: 'Start web' })).toBeDisabled()
    expect(screen.getByRole('button', { name: 'Stop web' })).toBeEnabled()
    expect(screen.getByRole('button', { name: 'Restart web' })).toBeEnabled()

    expect(screen.getByRole('button', { name: 'Start db' })).toBeEnabled()
    expect(screen.getByRole('button', { name: 'Stop db' })).toBeDisabled()
  })

  it('confirms an interrupting action before sending it', async () => {
    const user = userEvent.setup()
    const { onAction } = setup()

    await user.click(screen.getByRole('button', { name: 'Stop web' }))
    expect(screen.getByText('Stop the container web.')).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Stop' }))

    expect(onAction).toHaveBeenCalledWith(containers[0], 'stop')
  })

  it('sends a starting action without a confirmation', async () => {
    const user = userEvent.setup()
    const { onAction } = setup()

    await user.click(screen.getByRole('button', { name: 'Start db' }))

    expect(onAction).toHaveBeenCalledWith(containers[1], 'start')
  })

  it('selects a container for the log panel', async () => {
    const user = userEvent.setup()
    const { onSelect } = setup()

    await user.click(screen.getByRole('button', { name: 'db' }))

    expect(onSelect).toHaveBeenCalledWith(containers[1])
  })

  it('marks the selected container and the in-flight action', () => {
    setup({ selected: containers[1], busy: 'web:restart' })

    expect(screen.getByRole('button', { name: 'db' })).toHaveAttribute('aria-pressed', 'true')
    expect(screen.getByRole('button', { name: 'Restart web' })).toBeDisabled()
  })

  it('reports an empty table', () => {
    setup({ containers: [] })

    expect(screen.getByText('No containers are readable.')).toBeInTheDocument()
  })
})
