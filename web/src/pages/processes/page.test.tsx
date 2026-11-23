import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import type { ProcessesState } from '@/hooks/use-processes'
import type { ProcessInfo } from '@/lib/processes'
import { ProcessesPage, page } from './page'

const { useProcesses } = vi.hoisted(() => ({
  useProcesses: vi.fn<() => ProcessesState>(),
}))

vi.mock('@/hooks/use-processes', () => ({ useProcesses }))

const node: ProcessInfo = {
  pid: 812,
  parent_pid: 1,
  name: 'node',
  user: 'dev',
  state: 'running',
  cpu_seconds: 12,
  memory_bytes: 2048,
  command: 'node vite',
  started_at_unix: 1_700_000_000,
  niceness: 0,
}

const state: ProcessesState = {
  processes: [node],
  loading: false,
  error: null,
  busyPid: null,
  refresh: () => undefined,
  signal: async () => undefined,
  renice: async () => undefined,
}

describe('ProcessesPage', () => {
  it('composes the table inside a panel', () => {
    useProcesses.mockReturnValue(state)

    render(<ProcessesPage />)

    expect(screen.getByRole('heading', { name: 'Processes' })).toBeInTheDocument()
    expect(screen.getByRole('row', { name: /node/ })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Refresh' })).toBeInTheDocument()
  })

  it('registers its route and navigation entry', () => {
    expect(page.path).toBe('/processes')
    expect(page.title).toBe('Processes')
    expect(page.order).toBe(2)
  })
})
