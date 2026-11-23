import type { Meta, StoryObj } from '@storybook/react-vite'
import type { AgentInfo } from '@/lib/agents'
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

const meta: Meta<typeof AgentList> = {
  title: 'Building blocks/AgentList',
  component: AgentList,
}
export default meta

export const Default: StoryObj<typeof AgentList> = {
  args: {
    agents,
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
