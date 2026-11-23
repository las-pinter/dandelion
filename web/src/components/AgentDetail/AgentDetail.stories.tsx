import type { Meta, StoryObj } from '@storybook/react-vite'
import type { AgentInfo } from '@/lib/agents'
import { AgentDetail } from './AgentDetail'

const blocked: AgentInfo = {
  id: 'story-agent',
  title: 'claude in dandelion',
  cwd: '/home/dev/dandelion',
  labels: {},
  agent_kind: 'claude-code',
  agent_state: { state: 'blocked', blocked_reason: 'permission', progress: null },
  lifecycle: 'live',
  created_ms: 1_700_000_000_000,
  exit_code: null,
  finished_reason: null,
}

const meta: Meta<typeof AgentDetail> = {
  title: 'Building blocks/AgentDetail',
  component: AgentDetail,
}
export default meta

/** The pane needs a running backend, so Storybook shows a closed connection. */
export const Blocked: StoryObj<typeof AgentDetail> = {
  args: {
    agent: blocked,
    onAct: () => undefined,
  },
}

export const Working: StoryObj<typeof AgentDetail> = {
  args: {
    agent: {
      ...blocked,
      agent_state: { state: 'working', blocked_reason: null, progress: null },
    },
    onAct: () => undefined,
  },
}
