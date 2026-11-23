import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import type { ServicesState } from '@/hooks/use-services'
import type { ServicesSnapshot, UnitScope } from '@/lib/services'
import { ServicesPage, page } from './page'

const { useServices } = vi.hoisted(() => ({
  useServices: vi.fn<() => ServicesState>(),
}))

vi.mock('@/hooks/use-services', () => ({ useServices }))

const snapshot: ServicesSnapshot = {
  units: [
    {
      name: 'nginx.service',
      description: 'A high performance web server',
      load_state: 'loaded',
      active_state: 'active',
      sub_state: 'running',
      scope: 'system',
    },
  ],
  allowlist: [{ unit: 'nginx.service', verbs: ['restart'] }],
}

function state(overrides: Partial<ServicesState> = {}): ServicesState {
  return {
    snapshot,
    loading: false,
    error: null,
    busy: null,
    journal: [],
    journalLoading: false,
    journalError: null,
    refresh: () => undefined,
    act: async () => undefined,
    loadJournal: async () => undefined,
    ...overrides,
  }
}

describe('ServicesPage', () => {
  it('renders the table and loads a journal when a unit is selected', async () => {
    const user = userEvent.setup()
    const loadJournal = vi.fn<(scope: UnitScope, unit: string) => Promise<void>>()
    useServices.mockReturnValue(state({ loadJournal }))

    render(<ServicesPage />)

    expect(screen.getByRole('heading', { name: 'Services' })).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'nginx.service' }))

    expect(loadJournal).toHaveBeenCalledWith('system', 'nginx.service')
    expect(screen.getByText('nginx.service (system)')).toBeInTheDocument()
  })

  it('shows the snapshot error above the table', () => {
    useServices.mockReturnValue(state({ snapshot: null, error: 'systemd is unavailable' }))

    render(<ServicesPage />)

    expect(screen.getByRole('alert')).toHaveTextContent('systemd is unavailable')
  })

  it('registers its route and navigation entry', () => {
    expect(page.path).toBe('/services')
    expect(page.title).toBe('Services')
    expect(page.order).toBe(3)
  })
})
