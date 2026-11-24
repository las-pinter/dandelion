import { useMemo, useState } from 'react'
import { LayoutDashboard } from 'lucide-react'
import { mockAgents } from '@/app/mock-agents'
import type { PageDefinition } from '@/app/page-registry'
import { AgentRow } from '@/components/AgentRow/AgentRow'
import { MetricCard } from '@/components/MetricCard/MetricCard'
import { MetricChart } from '@/components/MetricChart/MetricChart'
import { NotificationsPanel } from '@/components/NotificationsPanel/NotificationsPanel'
import { Panel } from '@/components/Panel/Panel'
import { Button } from '@/components/ui/button'
import { useSystemMetrics } from '@/hooks/use-system-metrics'
import {
  formatBytes,
  formatRate,
  formatUptime,
  hottestTemperature,
  minuteSeries,
  rootDisk,
  secondSeries,
  type MetricsRange,
} from '@/lib/metrics'

const GIB = 1024 ** 3

export function DashboardPage() {
  const [range, setRange] = useState<MetricsRange>('1h')
  const { snapshot, seconds, minutes, connected, error } = useSystemMetrics(range)

  const series = useMemo(() => {
    if (range === '1h') {
      return {
        cpu: secondSeries(seconds, (point) => point.cpu_percent),
        memory: secondSeries(seconds, (point) => point.memory_used_bytes / GIB),
        load: secondSeries(seconds, (point) => point.load1),
      }
    }
    return {
      cpu: minuteSeries(minutes, (minute) => minute.cpu_percent.avg),
      memory: minuteSeries(minutes, (minute) => minute.memory_used_bytes.avg / GIB),
      load: minuteSeries(minutes, (minute) => minute.load1.avg),
    }
  }, [range, seconds, minutes])

  const disk = snapshot ? rootDisk(snapshot) : undefined
  const diskPercent =
    disk && disk.total_bytes > 0 ? (disk.used_bytes / disk.total_bytes) * 100 : undefined
  const hottest = snapshot ? hottestTemperature(snapshot) : undefined
  const rx = snapshot?.network.reduce((sum, interface_) => sum + interface_.rx_bytes_per_sec, 0)
  const tx = snapshot?.network.reduce((sum, interface_) => sum + interface_.tx_bytes_per_sec, 0)

  const metrics = [
    {
      label: 'CPU',
      value: snapshot ? Math.round(snapshot.cpu.usage_percent) : '—',
      unit: snapshot ? '%' : undefined,
      sparkline: snapshot ? sparkline(series.cpu) : undefined,
      hint: snapshot
        ? `${snapshot.cpu.per_core_percent.length} cores`
        : 'waiting for the first sample',
    },
    {
      label: 'Memory',
      value: snapshot ? (snapshot.memory.used_bytes / GIB).toFixed(1) : '—',
      unit: snapshot ? 'GiB' : undefined,
      sparkline: snapshot ? sparkline(series.memory) : undefined,
      hint: snapshot ? `of ${(snapshot.memory.total_bytes / GIB).toFixed(0)} GiB` : undefined,
    },
    {
      label: 'Disk',
      value: diskPercent === undefined ? '—' : Math.round(diskPercent),
      unit: diskPercent === undefined ? undefined : '%',
      hint: disk ? `of ${formatBytes(disk.total_bytes)}` : 'root filesystem',
    },
    {
      label: 'Load',
      value: snapshot ? snapshot.cpu.load.one.toFixed(2) : '—',
      sparkline: snapshot ? sparkline(series.load) : undefined,
      hint: '1 minute average',
    },
  ]

  const hostRows: [string, string][] = snapshot
    ? [
        ['Uptime', formatUptime(snapshot.uptime_secs)],
        ['Cores', String(snapshot.cpu.per_core_percent.length)],
        [
          'Load',
          `${snapshot.cpu.load.one.toFixed(2)} / ${snapshot.cpu.load.five.toFixed(2)} / ${snapshot.cpu.load.fifteen.toFixed(2)}`,
        ],
        [
          'Memory',
          `${formatBytes(snapshot.memory.used_bytes)} / ${formatBytes(snapshot.memory.total_bytes)}`,
        ],
        [
          'Root disk',
          disk ? `${formatBytes(disk.used_bytes)} / ${formatBytes(disk.total_bytes)}` : '—',
        ],
        ['Network', `${formatRate(rx ?? 0)} down · ${formatRate(tx ?? 0)} up`],
        ['Hottest', hottest === undefined ? '—' : `${hottest.toFixed(1)} °C`],
      ]
    : [
        ['Uptime', '—'],
        ['Cores', '—'],
        ['Load', '—'],
        ['Memory', '—'],
        ['Root disk', '—'],
        ['Network', '—'],
        ['Hottest', '—'],
      ]

  const waiting = mockAgents.filter((agent) => agent.status === 'waiting').length
  const attention = mockAgents.filter(
    (agent) => agent.status === 'waiting' || agent.status === 'error',
  )

  return (
    <div className="flex flex-col gap-4">
      {attention.length > 0 && (
        <Panel title="Needs you" description="Waiting or failed agents come first" status="waiting">
          <ul className="divide-y">
            {attention.map((agent) => (
              <li key={agent.name}>
                <AgentRow {...agent} />
              </li>
            ))}
          </ul>
        </Panel>
      )}
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-4">
        {metrics.map((metric) => (
          <MetricCard key={metric.label} {...metric} />
        ))}
      </div>
      <Panel
        title="Performance"
        description={
          connected ? 'Live · one sample per second' : (error ?? 'Connecting to the live stream…')
        }
        actions={
          <div className="flex items-center gap-1" role="group" aria-label="History window">
            {(['1h', '7d'] as const).map((option) => (
              <Button
                key={option}
                type="button"
                size="sm"
                variant={range === option ? 'default' : 'ghost'}
                aria-pressed={range === option}
                onClick={() => setRange(option)}
              >
                {option === '1h' ? '1 hour' : '7 days'}
              </Button>
            ))}
          </div>
        }
      >
        <div className="grid grid-cols-1 gap-4 lg:grid-cols-3">
          <MetricChart
            label="CPU"
            unit="%"
            points={series.cpu}
            domain={[0, 100]}
            color="var(--chart-1)"
          />
          <MetricChart label="Memory" unit="GiB" points={series.memory} color="var(--chart-2)" />
          <MetricChart label="Load" points={series.load} color="var(--chart-3)" />
        </div>
      </Panel>
      <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
        <Panel
          title="Agents"
          description={`${mockAgents.length} sample agents until the agents API lands`}
          status={waiting > 0 ? 'waiting' : 'idle'}
          actions={
            <span className="font-mono text-xs text-muted-foreground">
              {mockAgents.length} total
            </span>
          }
        >
          <ul className="divide-y">
            {mockAgents.map((agent) => (
              <li key={agent.name}>
                <AgentRow {...agent} />
              </li>
            ))}
          </ul>
        </Panel>
        <Panel title="Host" description="dandelion · Linux">
          <dl className="grid grid-cols-1 gap-x-6 gap-y-2 sm:grid-cols-2">
            {hostRows.map(([term, value]) => (
              <div key={term} className="flex min-h-11 items-center justify-between gap-3">
                <dt className="text-sm text-muted-foreground">{term}</dt>
                <dd className="truncate font-mono text-sm">{value}</dd>
              </div>
            ))}
          </dl>
        </Panel>
      </div>
      <NotificationsPanel />
    </div>
  )
}

function sparkline(points: { value: number }[]): number[] {
  return points.slice(-40).map((point) => point.value)
}

export const page: PageDefinition = {
  path: '/dashboard',
  title: 'Dashboard',
  icon: LayoutDashboard,
  order: 0,
  Component: DashboardPage,
}
