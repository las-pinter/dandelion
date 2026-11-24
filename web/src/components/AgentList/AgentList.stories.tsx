import type { Meta, StoryObj } from '@storybook/react-vite'
import type { AgentInfo, ClaudeAgent } from '@/lib/agents'
import { AgentList } from './AgentList'

const agents: AgentInfo[] = [
  {
    id: 't7',
    title: 'claude in dandelion',
    cwd: '/home/dev/dandelion',
    labels: {},
    agent_kind: 'claude-code',
    agent_state: { state: 'blocked', blocked_reason: 'permission', progress: null },
    lifecycle: 'live',
    created_ms: 1_700_000_000_000,
    exit_code: null,
    finished_reason: null,
  },
  {
    id: 't8',
    title: 'codex in worktree',
    cwd: '/home/dev/dandelion-worktree',
    labels: {},
    agent_kind: 'codex',
    agent_state: { state: 'working', blocked_reason: null, progress: null },
    lifecycle: 'live',
    created_ms: 1_700_000_100_000,
    exit_code: null,
    finished_reason: null,
  },
  {
    id: 't9',
    title: 'done agent',
    cwd: '/home/dev/other',
    labels: {},
    agent_kind: 'claude-code',
    agent_state: { state: 'done', blocked_reason: null, progress: null },
    lifecycle: 'finished',
    created_ms: 1_700_000_200_000,
    exit_code: 0,
    finished_reason: 'exited',
  },
]

const unmanaged: ClaudeAgent[] = [
  {
    terminal: null,
    sessionId: 'abcdef12-0000-0000-0000-000000000000',
    cwd: '/tmp/plain-shell',
    name: null,
    agent: null,
    version: '2.1.330',
    status: 'idle',
    waitingFor: null,
    updatedAt: null,
    usage: {
      sessionId: 'abcdef12-0000-0000-0000-000000000000',
      models: [],
      tokens: {
        inputTokens: 0,
        outputTokens: 0,
        cacheReadInputTokens: 0,
        cacheCreationInputTokens: 0,
      },
      costUsd: 0,
      unpricedModels: [],
      messages: 0,
    },
  },
]

const meta: Meta<typeof AgentList> = {
  title: 'Building blocks/AgentList',
  component: AgentList,
}
export default meta

export const Default: StoryObj<typeof AgentList> = {
  args: {
    agents,
    unmanaged,
    selectedId: 't7',
    onSelect: () => undefined,
  },
}

export const Empty: StoryObj<typeof AgentList> = {
  args: {
    agents: [],
    selectedId: null,
    onSelect: () => undefined,
  },
}
