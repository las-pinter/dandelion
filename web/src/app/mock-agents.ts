import type { AgentStatus } from '@/components/status'

/** Temporary agent fixtures until the server API exists. */
export interface MockAgent {
  name: string
  command: string
  status: AgentStatus
  detail: string
}

export const mockAgents: MockAgent[] = [
  { name: 'herdr', command: 'herdr attach lane-9', status: 'working', detail: '12m' },
  { name: 'review-bot', command: 'claude --agent reviewer', status: 'waiting', detail: '2m' },
  { name: 'backup', command: 'restic backup /home', status: 'idle', detail: '—' },
  { name: 'deploy', command: 'just deploy', status: 'done', detail: '1h' },
  { name: 'scraper', command: 'python crawl.py', status: 'error', detail: '4m' },
]
