import { LayoutDashboard } from 'lucide-react'
import { mockAgents } from '@/app/mock-agents'
import type { PageDefinition } from '@/app/page-registry'
import { AgentRow } from '@/components/AgentRow/AgentRow'
import { MetricCard } from '@/components/MetricCard/MetricCard'
import { Panel } from '@/components/Panel/Panel'

interface MockMetric {
  label: string
  value: string
  unit?: string
  sparkline: number[]
  hint: string
}

const metrics: MockMetric[] = [
  {
    label: 'CPU',
    value: '42',
    unit: '%',
    sparkline: [28, 35, 31, 40, 38, 45, 42],
    hint: '8 cores · 3.4 GHz',
  },
  {
    label: 'Memory',
    value: '15.2',
    unit: 'GiB',
    sparkline: [12.1, 12.6, 13.2, 13.8, 14.5, 14.9, 15.2],
    hint: 'of 32 GiB',
  },
  {
    label: 'Disk',
    value: '61',
    unit: '%',
    sparkline: [58, 58, 59, 59, 60, 60, 61],
    hint: 'of 1.8 TiB',
  },
  {
    label: 'Load',
    value: '2.41',
    sparkline: [1.8, 2.2, 2.0, 2.6, 2.3, 2.7, 2.41],
    hint: '1 minute average',
  },
]

const hostRows = [
  ['Uptime', '12d 04h'],
  ['Kernel', '6.17.0-35'],
  ['Services', '18 / 20'],
  ['Containers', '6 running'],
  ['Sessions', '3 agents'],
  ['TLS', 'valid · 83d'],
]

function DashboardPage() {
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
      <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
        <Panel
          title="Agents"
          description={`${mockAgents.length} tracked on this host`}
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
    </div>
  )
}

export const page: PageDefinition = {
  path: '/dashboard',
  title: 'Dashboard',
  icon: LayoutDashboard,
  order: 0,
  Component: DashboardPage,
}
