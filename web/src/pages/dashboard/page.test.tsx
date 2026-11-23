import { fireEvent, render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { SystemMetricsState } from '@/hooks/use-system-metrics'
import type { MetricsRange } from '@/lib/metrics'
import { DashboardPage } from './page'

const { useSystemMetrics } = vi.hoisted(() => ({
  useSystemMetrics: vi.fn<(range: MetricsRange) => SystemMetricsState>(),
}))

vi.mock('@/hooks/use-system-metrics', () => ({ useSystemMetrics }))

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
        used_bytes: 256 * 1024 ** 3,
        available_bytes: 256 * 1024 ** 3,
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

beforeEach(() => {
  useSystemMetrics.mockReturnValue(state)
})

describe('DashboardPage', () => {
  it('shows live metrics, charts, and host facts', () => {
    render(<DashboardPage />)

    expect(screen.getByText('8 cores')).toBeInTheDocument()
    expect(screen.getByText('of 8 GiB')).toBeInTheDocument()
    expect(screen.getByText('of 512.0 GiB')).toBeInTheDocument()
    expect(screen.getByText('1 minute average')).toBeInTheDocument()
    expect(screen.getByRole('img', { name: /CPU history/ })).toBeInTheDocument()
    expect(screen.getByRole('img', { name: /Memory history/ })).toBeInTheDocument()
    expect(screen.getByText('1d 04h')).toBeInTheDocument()
    expect(screen.getByText('Live · one sample per second')).toBeInTheDocument()
  })

  it('switches the history window', () => {
    render(<DashboardPage />)

    fireEvent.click(screen.getByRole('button', { name: '7 days' }))

    expect(useSystemMetrics).toHaveBeenLastCalledWith('7d')
  })
})
