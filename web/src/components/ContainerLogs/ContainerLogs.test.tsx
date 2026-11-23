import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import type { ContainerInfo, ContainerLogLine } from '@/lib/containers'
import { ContainerLogs } from './ContainerLogs'

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

const lines: ContainerLogLine[] = [
  { stream: 'stdout', message: '2026-11-23T12:34:56Z listening on :80' },
  { stream: 'stderr', message: 'plain line without a timestamp' },
]

function setup(overrides: Partial<Parameters<typeof ContainerLogs>[0]> = {}) {
  const onRefresh = vi.fn<() => void>()
  render(<ContainerLogs container={web} lines={lines} onRefresh={onRefresh} {...overrides} />)
  return { onRefresh }
}

describe('ContainerLogs', () => {
  it('asks for a selection before any logs', () => {
    setup({ container: null, lines: [] })

    expect(screen.getByText('Select a container above to read its logs.')).toBeInTheDocument()
  })

  it('splits the Docker timestamp from each line', () => {
    setup()

    expect(screen.getByText('listening on :80')).toBeInTheDocument()
    expect(screen.getByText('plain line without a timestamp')).toBeInTheDocument()
    expect(screen.getByText('web · nginx:1')).toBeInTheDocument()
  })

  it('shows an error instead of the lines', () => {
    setup({ error: 'Docker is unavailable' })

    expect(screen.getByRole('alert')).toHaveTextContent('Docker is unavailable')
    expect(screen.queryByText('listening on :80')).not.toBeInTheDocument()
  })

  it('reports an empty tail and reloads on demand', async () => {
    const user = userEvent.setup()
    const { onRefresh } = setup({ lines: [] })

    expect(screen.getByText('No log lines.')).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Refresh' }))

    expect(onRefresh).toHaveBeenCalled()
  })
})
