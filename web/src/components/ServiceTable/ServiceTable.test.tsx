import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import type { AllowRule, UnitAction, UnitInfo } from '@/lib/services'
import { ServiceTable } from './ServiceTable'

const units: UnitInfo[] = [
  {
    name: 'app.service',
    description: 'The application server',
    load_state: 'loaded',
    active_state: 'active',
    sub_state: 'running',
    scope: 'user',
  },
  {
    name: 'nginx.service',
    description: 'A high performance web server',
    load_state: 'loaded',
    active_state: 'active',
    sub_state: 'running',
    scope: 'system',
  },
  {
    name: 'backup.timer',
    description: 'Nightly backup',
    load_state: 'loaded',
    active_state: 'inactive',
    sub_state: 'dead',
    scope: 'system',
  },
]

const rules: AllowRule[] = [{ unit: 'nginx.service', verbs: ['restart'] }]

function setup(overrides: Partial<Parameters<typeof ServiceTable>[0]> = {}) {
  const onSelect = vi.fn<(unit: UnitInfo) => void>()
  const onAction = vi.fn<(unit: UnitInfo, action: UnitAction) => void>()
  render(
    <ServiceTable
      units={units}
      rules={rules}
      selected={null}
      onSelect={onSelect}
      onAction={onAction}
      {...overrides}
    />,
  )
  return { onSelect, onAction }
}

describe('ServiceTable', () => {
  it('renders the unit name, scope and state', () => {
    setup()

    expect(screen.getByRole('button', { name: 'nginx.service' })).toBeInTheDocument()
    expect(screen.getByText('A high performance web server')).toBeInTheDocument()
    expect(screen.getByText('3 of 3 units')).toBeInTheDocument()
  })

  it('filters by text, scope, type and state', async () => {
    const user = userEvent.setup()
    setup()

    await user.type(screen.getByLabelText('Search units'), 'backup')
    expect(screen.getByRole('button', { name: 'backup.timer' })).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'nginx.service' })).not.toBeInTheDocument()

    await user.clear(screen.getByLabelText('Search units'))
    await user.selectOptions(screen.getByLabelText('Scope'), 'user')
    expect(screen.getByRole('button', { name: 'app.service' })).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'nginx.service' })).not.toBeInTheDocument()

    await user.selectOptions(screen.getByLabelText('Scope'), 'all')
    await user.selectOptions(screen.getByLabelText('Type'), 'timer')
    expect(screen.getByRole('button', { name: 'backup.timer' })).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'app.service' })).not.toBeInTheDocument()

    await user.selectOptions(screen.getByLabelText('Type'), 'all')
    await user.selectOptions(screen.getByLabelText('State'), 'active')
    expect(screen.queryByRole('button', { name: 'backup.timer' })).not.toBeInTheDocument()
  })

  it('gates system actions on the allowlist', () => {
    setup()

    expect(screen.getByRole('button', { name: 'Restart nginx.service' })).toBeEnabled()
    expect(screen.getByRole('button', { name: 'Stop nginx.service' })).toBeDisabled()
    expect(screen.getByRole('button', { name: 'Restart backup.timer' })).toBeDisabled()
    expect(screen.getByRole('button', { name: 'Stop app.service' })).toBeEnabled()
  })

  it('confirms an interrupting action before sending it', async () => {
    const user = userEvent.setup()
    const { onAction } = setup()

    await user.click(screen.getByRole('button', { name: 'Restart nginx.service' }))
    expect(screen.getByText(/This changes the system unit nginx.service/)).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Restart' }))

    expect(onAction).toHaveBeenCalledWith(units[1], 'restart')
  })

  it('sends a starting action without a confirmation', async () => {
    const user = userEvent.setup()
    const { onAction } = setup()

    await user.click(screen.getByRole('button', { name: 'Start app.service' }))

    expect(onAction).toHaveBeenCalledWith(units[0], 'start')
  })

  it('selects a unit for the journal panel', async () => {
    const user = userEvent.setup()
    const { onSelect } = setup()

    await user.click(screen.getByRole('button', { name: 'backup.timer' }))

    expect(onSelect).toHaveBeenCalledWith(units[2])
  })

  it('marks the selected unit and the in-flight action', () => {
    setup({
      selected: units[1],
      busy: 'system:nginx.service:restart',
      rules: [],
    })

    expect(screen.getByRole('button', { name: 'nginx.service' })).toHaveAttribute(
      'aria-pressed',
      'true',
    )
    expect(screen.getByRole('button', { name: 'Restart nginx.service' })).toBeDisabled()
  })

  it('reports an empty table', () => {
    setup({ units: [] })

    expect(screen.getByText('No units are readable.')).toBeInTheDocument()
  })
})
