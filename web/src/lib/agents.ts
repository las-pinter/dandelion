import type { components } from '@/api/gen/schema'
import type { AgentStatus as UiStatus } from '@/components/status'

export type AgentsSnapshot = components['schemas']['AgentsSnapshot']
export type AgentInfo = components['schemas']['TerminalInfo']
export type AgentWord = components['schemas']['AgentState']
export type AgentStatusInfo = components['schemas']['AgentStatus']
export type BlockedReason = components['schemas']['BlockedReason']
export type AgentAction = components['schemas']['AgentAction']
export type SpawnAgentRequest = components['schemas']['SpawnAgentRequest']

/** The pane socket for one agent on this origin. */
export function agentSocketUrl(id: string): string {
  const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
  return `${protocol}//${window.location.host}/api/agents/${encodeURIComponent(id)}/ws`
}

/** A row label: the terminal title, then its backend id. */
export function agentLabel(agent: AgentInfo): string {
  return agent.title || agent.id
}

/** Maps the backend's agent state onto the five UI states. */
export function uiStatus(agent: AgentInfo): UiStatus {
  switch (agent.agent_state?.state) {
    case 'working':
      return 'working'
    case 'blocked':
      return 'waiting'
    case 'idle':
      return 'idle'
    case 'done':
      return 'done'
    case 'error':
      return 'error'
    default:
      return 'idle'
  }
}

/** The blocked reason as a short phrase, or null when nothing waits. */
export function blockedLabel(status: AgentStatusInfo | null | undefined): string | null {
  switch (status?.blocked_reason) {
    case 'permission':
      return 'needs permission'
    case 'question':
      return 'asks a question'
    case 'auth':
      return 'needs sign-in'
    case 'other':
      return 'blocked'
    default:
      return null
  }
}
