import { useMemo, useState } from 'react'
import { Link } from 'react-router'
import { AgentRow } from '@/components/AgentRow/AgentRow'
import { MetricCard } from '@/components/MetricCard/MetricCard'
import { MetricChart } from '@/components/MetricChart/MetricChart'
import { Panel } from '@/components/Panel/Panel'
import type { AgentStatus } from '@/components/status'
import { Button } from '@/components/ui/button'
import { useAgents } from '@/hooks/use-agents'
import { useSystemMetrics } from '@/hooks/use-system-metrics'
import {
  agentLabel,
  blockedLabel,
  claudeLabel,
  claudeUiStatus,
  uiStatus,
  unmanagedSessions,
  waitingLabel,
  type AgentInfo,
  type ClaudeAgent,
} from '@/lib/agents'
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

/** One dashboard row: a live agent, or a Claude session with no terminal. */
interface AgentLine {
  key: string
  name: string
  status: AgentStatus
  command: string
  detail?: string
}

function managedLine(agent: AgentInfo): AgentLine {
  return {
    key: agent.id,
    name: agentLabel(agent),
    status: uiStatus(agent),
    command: `${agent.agent_kind ?? 'agent'} · ${agent.cwd ?? 'unknown directory'}`,
    detail: blockedLabel(agent.agent_state) ?? agent.id,
  }
}

function sessionLine(session: ClaudeAgent): AgentLine {
  const waiting = waitingLabel(session.waitingFor)
  return {
    key: `claude:${session.sessionId}`,
    name: claudeLabel(session),
    status: claudeUiStatus(session),
    command: `unmanaged claude · ${session.cwd || 'unknown directory'}`,
    detail: waiting ? `waiting for ${waiting}` : undefined,
  }
}

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
  // Fullness the way `df` reports it: the blocks reserved for root are not
  // available to the user, so a share of the total understates the gauge.
  const diskCapacity = disk ? disk.used_bytes + disk.available_bytes : 0
  const diskPercent = disk && diskCapacity > 0 ? (disk.used_bytes / diskCapacity) * 100 : undefined
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

  const agentsState = useAgents()
  const agents = useMemo(() => {
    const listing = agentsState.snapshot
    return [
      ...(listing?.agents ?? []).map(managedLine),
      ...unmanagedSessions(listing?.claude).map(sessionLine),
    ]
  }, [agentsState.snapshot])
  const waiting = agents.filter((line) => line.status === 'waiting').length
  const attention = agents.filter((line) => line.status === 'waiting' || line.status === 'error')
  const agentsNote = agentsState.error
    ? agentsState.error
    : agentsState.snapshot && !agentsState.snapshot.available
      ? 'No terminal backend is reachable'
      : null

  return (
    <div className="flex flex-col gap-4">
      {attention.length > 0 && (
        <Panel title="Needs you" description="Waiting or failed agents come first" status="waiting">
          <ul className="divide-y">
            {attention.map((line) => (
              <li key={line.key}>
                <AgentRow
                  name={line.name}
                  status={line.status}
                  command={line.command}
                  detail={line.detail}
                />
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
          description={agentsNote ?? 'Live from the terminal backend'}
          status={waiting > 0 ? 'waiting' : 'idle'}
          actions={
            <div className="flex items-center gap-3">
              <span className="font-mono text-xs text-muted-foreground">{agents.length} total</span>
              <Link
                to="/agents"
                className="text-xs text-muted-foreground underline-offset-4 hover:text-foreground hover:underline"
              >
                Open agents
              </Link>
            </div>
          }
        >
          {agentsState.loading && !agentsState.snapshot ? (
            <p className="py-6 text-center text-sm text-muted-foreground">Reading agents…</p>
          ) : agents.length === 0 ? (
            <p className="py-6 text-center text-sm text-muted-foreground">
              {agentsNote ?? 'No agents are running.'}
            </p>
          ) : (
            <ul className="divide-y">
              {agents.map((line) => (
                <li key={line.key}>
                  <AgentRow
                    name={line.name}
                    status={line.status}
                    command={line.command}
                    detail={line.detail}
                  />
                </li>
              ))}
            </ul>
          )}
        </Panel>
        <Panel title="Host" description="This machine · Linux">
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
    </div>
  )
}

function sparkline(points: { value: number }[]): number[] {
  return points.slice(-40).map((point) => point.value)
}
