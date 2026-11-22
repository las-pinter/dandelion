import { Bot } from 'lucide-react'
import { mockAgents } from '@/app/mock-agents'
import type { PageDefinition } from '@/app/page-registry'
import { AgentRow } from '@/components/AgentRow/AgentRow'
import { Panel } from '@/components/Panel/Panel'
import { StatusDot } from '@/components/StatusDot/StatusDot'
import { AGENT_STATUSES, type AgentStatus } from '@/components/status'

function countByStatus(status: AgentStatus) {
  return mockAgents.filter((agent) => agent.status === status).length
}

function AgentsPage() {
  return (
    <div className="flex flex-col gap-4">
      <Panel title="By state" description="Every agent state carries a text label">
        <ul className="grid grid-cols-2 gap-2 sm:grid-cols-3 lg:grid-cols-5">
          {AGENT_STATUSES.map((status) => (
            <li
              key={status}
              className="flex min-h-11 items-center justify-between gap-2 rounded-xl border px-3"
            >
              <StatusDot status={status} withLabel />
              <span className="font-mono text-sm tabular-nums">{countByStatus(status)}</span>
            </li>
          ))}
        </ul>
      </Panel>
      <Panel title="Agents" description="Waiting needs the owner first">
        <ul className="divide-y">
          {mockAgents.map((agent) => (
            <li key={agent.name}>
              <AgentRow {...agent} />
            </li>
          ))}
        </ul>
      </Panel>
    </div>
  )
}

export const page: PageDefinition = {
  path: '/agents',
  title: 'Agents',
  icon: Bot,
  order: 1,
  badge: '1',
  Component: AgentsPage,
}
