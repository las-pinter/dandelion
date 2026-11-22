/**
 * The five agent states from docs/design.md. Color is never the only signal:
 * every state also has a text label.
 */
export type AgentStatus = 'working' | 'waiting' | 'idle' | 'done' | 'error'

export interface StatusMeta {
  label: string
  dot: string
  text: string
  border: string
  /** Motion belongs only to states that are alive right now. */
  motion?: string
}

export const STATUS_META: Record<AgentStatus, StatusMeta> = {
  working: {
    label: 'Working',
    dot: 'bg-status-working',
    text: 'text-status-working',
    border: 'border-status-working/40',
    motion: 'animate-pulse',
  },
  waiting: {
    label: 'Waiting',
    dot: 'bg-status-waiting',
    text: 'text-status-waiting',
    border: 'border-status-waiting/40',
    motion: 'animate-pulse [animation-duration:2.5s]',
  },
  idle: {
    label: 'Idle',
    dot: 'bg-status-idle',
    text: 'text-status-idle',
    border: 'border-status-idle/40',
  },
  done: {
    label: 'Done',
    dot: 'bg-status-done',
    text: 'text-status-done',
    border: 'border-status-done/40',
  },
  error: {
    label: 'Error',
    dot: 'bg-status-error',
    text: 'text-status-error',
    border: 'border-status-error/40',
  },
}

export const AGENT_STATUSES: AgentStatus[] = ['working', 'waiting', 'idle', 'done', 'error']
