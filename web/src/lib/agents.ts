import type { components } from '@/api/gen/schema'
import type { AgentStatus as UiStatus } from '@/components/status'

export type AgentsSnapshot = components['schemas']['AgentsSnapshot']
export type AgentInfo = components['schemas']['TerminalInfo']
export type AgentWord = components['schemas']['AgentState']
export type AgentStatusInfo = components['schemas']['AgentStatus']
export type BlockedReason = components['schemas']['BlockedReason']
export type AgentAction = components['schemas']['AgentAction']
export type SpawnAgentRequest = components['schemas']['SpawnAgentRequest']
export type ClaudeAgent = components['schemas']['ClaudeAgent']

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

/** The Claude Code session behind a terminal, when the monitor knows one. */
export function claudeFor(
  sessions: ClaudeAgent[] | undefined,
  terminal: string | null | undefined,
): ClaudeAgent | null {
  if (!terminal) return null
  return sessions?.find((session) => session.terminal === terminal) ?? null
}

/** A compact token count: 950, 12.3k, 4.5M. */
export function formatTokens(count: number): string {
  // Round up a step early where `toFixed(1)` would print 1000.0k.
  if (count >= 999_950) return `${(count / 1_000_000).toFixed(1)}M`
  if (count >= 1_000) return `${(count / 1_000).toFixed(1)}k`
  return `${count}`
}

export interface ClaudeSummary {
  /** The full session id; the card truncates it for display. */
  sessionId: string
  /** The first model in the usage list, or `unknown`. */
  model: string
  /** All token classes, compacted. */
  tokens: string
  /** Estimated cost; `—` before the first response, `unpriced` without a price. */
  cost: string
  /** What the session waits for, when it is blocked. */
  waiting: string | null
}

/** Display strings for one Claude session's card. */
export function claudeSummary(session: ClaudeAgent): ClaudeSummary {
  const { tokens, messages, costUsd, models, unpricedModels } = session.usage
  const total =
    tokens.inputTokens +
    tokens.outputTokens +
    tokens.cacheReadInputTokens +
    tokens.cacheCreationInputTokens
  const priced = models.some((model) => model.costUsd != null)
  const partial = unpricedModels.length > 0 || models.some((model) => model.costUsd == null)

  return {
    sessionId: session.sessionId,
    model: models[0]?.model ?? 'unknown',
    tokens: formatTokens(total),
    cost:
      messages === 0 ? '—' : priced ? `~$${costUsd.toFixed(2)}${partial ? '+' : ''}` : 'unpriced',
    waiting: waitingLabel(session.waitingFor),
  }
}

function waitingLabel(waitingFor: string | null | undefined): string | null {
  switch (waitingFor) {
    case 'permission prompt':
      return 'a permission prompt'
    case 'input needed':
      return 'input'
    case 'dialog open':
      return 'an open dialog'
    default:
      return waitingFor ?? null
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
