import { StatusPill } from '@/components/StatusPill/StatusPill'
import {
  agentLabel,
  blockedLabel,
  claudeLabel,
  claudeUiStatus,
  uiStatus,
  type AgentInfo,
  type ClaudeAgent,
} from '@/lib/agents'
import { SELECT_ROW_CLASS } from '@/lib/row-control'

export interface AgentListProps {
  agents: AgentInfo[]
  /** Claude sessions outside herdr; they are listed but cannot be controlled. */
  unmanaged?: ClaudeAgent[]
  /** The agent whose pane is open, if any. */
  selectedId: string | null
  onSelect: (id: string) => void
}

/** Every agent the backend reports, with its state and working directory. */
export function AgentList({ agents, unmanaged = [], selectedId, onSelect }: AgentListProps) {
  return (
    <div className="overflow-x-auto">
      <table className="w-full min-w-[40rem] text-left text-sm">
        <caption className="sr-only">Live agents</caption>
        <thead className="bg-muted/60 text-xs font-medium text-muted-foreground">
          <tr className="border-b">
            <th scope="col" className="py-2 pr-3 font-medium">
              Agent
            </th>
            <th scope="col" className="py-2 pr-3 font-medium">
              State
            </th>
            <th scope="col" className="py-2 font-medium">
              Directory
            </th>
          </tr>
        </thead>
        <tbody className="divide-y">
          {agents.map((agent) => (
            <tr
              key={agent.id}
              aria-current={selectedId === agent.id ? 'true' : undefined}
              className="hover:bg-muted/40 aria-[current=true]:bg-muted/60"
            >
              <td className="py-2 pr-3">
                <button
                  type="button"
                  aria-pressed={selectedId === agent.id}
                  className={SELECT_ROW_CLASS}
                  onClick={() => onSelect(agent.id)}
                >
                  {agentLabel(agent)}
                </button>
                <span className="block text-xs text-muted-foreground">
                  {agent.agent_kind ?? 'agent'} · {agent.id}
                </span>
              </td>
              <td className="py-2 pr-3">
                <StatusPill status={uiStatus(agent)} />
                {blockedLabel(agent.agent_state) && (
                  <span className="block text-xs text-status-waiting">
                    {blockedLabel(agent.agent_state)}
                  </span>
                )}
              </td>
              <td className="max-w-[20rem] truncate py-2 font-mono text-xs text-muted-foreground">
                {agent.cwd ?? '—'}
              </td>
            </tr>
          ))}
          {unmanaged.map((session) => (
            <tr key={session.sessionId} className="hover:bg-muted/40">
              <td className="py-2 pr-3">
                <button
                  type="button"
                  aria-pressed={selectedId === `claude:${session.sessionId}`}
                  className={SELECT_ROW_CLASS}
                  onClick={() => onSelect(`claude:${session.sessionId}`)}
                >
                  {claudeLabel(session)}
                </button>
                <span className="block text-xs text-muted-foreground">
                  {session.agent ?? 'agent'} · {session.sessionId.slice(0, 8)} ·{' '}
                  <span className="text-status-waiting">unmanaged</span>
                </span>
              </td>
              <td className="py-2 pr-3">
                <StatusPill status={claudeUiStatus(session)} />
              </td>
              <td className="max-w-[20rem] truncate py-2 font-mono text-xs text-muted-foreground">
                {session.cwd || '—'}
              </td>
            </tr>
          ))}
          {agents.length === 0 && unmanaged.length === 0 && (
            <tr>
              <td colSpan={3} className="py-8 text-center text-sm text-muted-foreground">
                No agent is running.
              </td>
            </tr>
          )}
        </tbody>
      </table>
    </div>
  )
}
