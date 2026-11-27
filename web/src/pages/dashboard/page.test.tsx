import { fireEvent, render, screen, within } from '@testing-library/react'
import { MemoryRouter } from 'react-router'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { AgentsState } from '@/hooks/use-agents'
import type { SystemMetricsState } from '@/hooks/use-system-metrics'
import type { AgentsSnapshot } from '@/lib/agents'
import type { MetricsRange } from '@/lib/metrics'
import { DashboardPage } from './screen'

const { useSystemMetrics, useAgents } = vi.hoisted(() => ({
  useSystemMetrics: vi.fn<(range: MetricsRange) => SystemMetricsState>(),
  useAgents: vi.fn<() => AgentsState>(),
}))

vi.mock('@/hooks/use-system-metrics', () => ({ useSystemMetrics }))
vi.mock('@/hooks/use-agents', () => ({ useAgents }))

const point = {
  cpu_percent: 42,
  memory_used_bytes: 2 * 1024 ** 3,
  memory_total_bytes: 8 * 1024 ** 3,
  load1: 0.5,
  rx_bytes_per_sec: 0,
  tx_bytes_per_sec: 0,
  read_bytes_per_sec: 0,
  write_bytes_per_sec: 0,
}

const state: SystemMetricsState = {
  snapshot: {
    timestamp_ms: 4,
    uptime_secs: 86_400 + 4 * 3_600,
    cpu: {
      usage_percent: 42,
      per_core_percent: [40, 41, 42, 43, 44, 45, 46, 47],
      load: { one: 0.5, five: 0.4, fifteen: 0.3 },
    },
    memory: {
      total_bytes: 8 * 1024 ** 3,
      used_bytes: 2 * 1024 ** 3,
      available_bytes: 6 * 1024 ** 3,
      cache_bytes: 0,
      swap_total_bytes: 0,
      swap_used_bytes: 0,
    },
    disks: [
      {
        device: '/dev/sda2',
        device_name: 'sda2',
        mount_point: '/',
        filesystem: 'ext4',
        total_bytes: 512 * 1024 ** 3,
        used_bytes: 400 * 1024 ** 3,
        available_bytes: 64 * 1024 ** 3,
        read_bytes_per_sec: 0,
        write_bytes_per_sec: 0,
      },
    ],
    network: [{ name: 'eth0', rx_bytes_per_sec: 1024, tx_bytes_per_sec: 2048 }],
    temperatures: [{ chip: 'coretemp', label: 'Package', celsius: 45 }],
    top_processes_by_cpu: [],
    top_processes_by_memory: [],
  },
  seconds: [
    { ...point, timestamp_ms: 1 },
    { ...point, timestamp_ms: 2 },
    { ...point, timestamp_ms: 3 },
  ],
  minutes: [],
  connected: true,
  error: null,
}

const session = '0f3c2b1a-aaaa-bbbb-cccc-0123456789ab'
const agentsSnapshot: AgentsSnapshot = {
  available: true,
  backend: { kind: 'herdr_core', available: true, access: 'read_write', detail: null },
  agents: [
    {
      id: 't7',
      title: 'claude in dandelion',
      cwd: '/home/dev/dandelion',
      labels: {},
      agent_kind: 'claude-code',
      agent_state: { state: 'blocked', blocked_reason: 'permission', progress: null },
      lifecycle: 'live',
      created_ms: 1,
      exit_code: null,
      finished_reason: null,
    },
    {
      id: 't8',
      title: 'quiet shell',
      cwd: '/tmp',
      labels: {},
      agent_kind: null,
      agent_state: { state: 'idle', blocked_reason: null, progress: null },
      lifecycle: 'live',
      created_ms: 2,
      exit_code: null,
      finished_reason: null,
    },
  ],
  claude: [
    {
      terminal: null,
      sessionId: session,
      cwd: '/tmp/unmanaged',
      name: null,
      agent: null,
      version: '2.1.330',
      status: 'waiting',
      waitingFor: 'input needed',
      updatedAt: 1,
      usage: {
        sessionId: session,
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
    },
  ],
}

function agentsState(overrides: Partial<AgentsState> = {}): AgentsState {
  return {
    snapshot: agentsSnapshot,
    loading: false,
    error: null,
    busy: null,
    refresh: vi.fn<() => void>(),
    act: vi.fn<AgentsState['act']>(async () => undefined),
    spawn: vi.fn<AgentsState['spawn']>(async () => null),
    ...overrides,
  }
}

/** The page links to the Agents page, so it renders inside a router. */
function view() {
  return render(
    <MemoryRouter>
      <DashboardPage />
    </MemoryRouter>,
  )
}

beforeEach(() => {
  useSystemMetrics.mockReturnValue(state)
  useAgents.mockReturnValue(agentsState())
})

describe('DashboardPage', () => {
  it('shows live metrics, charts, and host facts', () => {
    view()

    expect(screen.getByText('8 cores')).toBeInTheDocument()
    expect(screen.getByText('of 8 GiB')).toBeInTheDocument()
    expect(screen.getByText('of 512.0 GiB')).toBeInTheDocument()
    // 400 GiB used of 464 GiB the user can fill: the `df` share, not 400/512.
    const diskCard = screen.getByText('Disk').closest('div') as HTMLElement
    expect(within(diskCard).getByText('86')).toBeInTheDocument()
    expect(screen.getByText('1 minute average')).toBeInTheDocument()
    expect(screen.getByRole('img', { name: /CPU history/ })).toBeInTheDocument()
    expect(screen.getByRole('img', { name: /Memory history/ })).toBeInTheDocument()
    expect(screen.getByText('1d 04h')).toBeInTheDocument()
    expect(screen.getByText('Live · one sample per second')).toBeInTheDocument()
  })

  it('switches the history window', () => {
    view()

    fireEvent.click(screen.getByRole('button', { name: '7 days' }))

    expect(useSystemMetrics).toHaveBeenLastCalledWith('7d')
  })

  it('shows the live agents and who needs attention', () => {
    view()

    const needs = screen.getByText('Needs you').closest('section') as HTMLElement
    expect(within(needs).getByText('claude in dandelion')).toBeInTheDocument()
    expect(within(needs).getByText('needs permission')).toBeInTheDocument()
    // The unmanaged session waits too, so it is a row here as well.
    expect(within(needs).getByText('session 0f3c2b1a')).toBeInTheDocument()
    expect(within(needs).getByText('waiting for input')).toBeInTheDocument()

    const list = screen
      .getByText('Live from the terminal backend')
      .closest('section') as HTMLElement
    expect(within(list).getByText('quiet shell')).toBeInTheDocument()
    expect(within(list).getByText('3 total')).toBeInTheDocument()
    expect(within(list).getByRole('link', { name: 'Open agents' })).toHaveAttribute(
      'href',
      '/agents',
    )
  })

  it('says when no agents are running', () => {
    useAgents.mockReturnValue(
      agentsState({ snapshot: { available: true, backend: null, agents: [], claude: [] } }),
    )

    view()

    expect(screen.getByText('No agents are running.')).toBeInTheDocument()
    expect(screen.queryByText('Needs you')).toBeNull()
  })

  it('says when no terminal backend answers', () => {
    useAgents.mockReturnValue(
      agentsState({ snapshot: { available: false, backend: null, agents: [], claude: [] } }),
    )

    view()

    expect(screen.getAllByText('No terminal backend is reachable').length).toBeGreaterThan(0)
  })
})
