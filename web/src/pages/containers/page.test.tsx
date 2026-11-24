import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import type { ContainersState } from '@/hooks/use-containers'
import type { ContainerInfo, ContainersSnapshot } from '@/lib/containers'
import { ContainersPage, page } from './page'

const { useContainers } = vi.hoisted(() => ({
  useContainers: vi.fn<() => ContainersState>(),
}))

vi.mock('@/hooks/use-containers', () => ({ useContainers }))

const web: ContainerInfo = {
  id: 'aaa111bbb222',
  name: 'web',
  image: 'nginx:1',
  command: 'nginx -g daemon off;',
  state: 'running',
  status: 'Up 2 minutes',
  created_at_unix: 1_700_000_000,
  labels: {},
}

const snapshot: ContainersSnapshot = {
  availability: { available: true, reason: null, server_version: '29.6.2' },
  containers: [web],
}

function state(overrides: Partial<ContainersState> = {}): ContainersState {
  return {
    snapshot,
    loading: false,
    error: null,
    busy: null,
    logs: [],
    logsLoading: false,
    logsError: null,
    refresh: () => undefined,
    act: async () => undefined,
    loadLogs: async () => undefined,
    ...overrides,
  }
}

describe('ContainersPage', () => {
  it('renders the table and loads logs when a container is selected', async () => {
    const user = userEvent.setup()
    const loadLogs = vi.fn<(container: string) => Promise<void>>()
    useContainers.mockReturnValue(state({ loadLogs }))

    render(<ContainersPage />)

    expect(screen.getByRole('heading', { name: 'Containers' })).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'web' }))

    expect(loadLogs).toHaveBeenCalledWith('web')
    expect(screen.getByText('web · nginx:1')).toBeInTheDocument()
  })

  it('shows the snapshot error above the table', () => {
    useContainers.mockReturnValue(state({ snapshot: null, error: 'Docker API error: boom' }))

    render(<ContainersPage />)

    expect(screen.getByRole('alert')).toHaveTextContent('Docker API error: boom')
  })

  it('explains an unavailable daemon', () => {
    useContainers.mockReturnValue(
      state({
        snapshot: {
          availability: { available: false, reason: 'no Docker socket', server_version: null },
          containers: [],
        },
      }),
    )

    render(<ContainersPage />)

    expect(screen.getByText(/Docker is unavailable: no Docker socket/)).toBeInTheDocument()
  })

  it('registers its route and navigation entry', () => {
    expect(page.path).toBe('/containers')
    expect(page.title).toBe('Containers')
    expect(page.order).toBe(4)
  })
})
