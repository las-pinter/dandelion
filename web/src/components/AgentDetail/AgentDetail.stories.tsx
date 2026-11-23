import type { Meta, StoryObj } from '@storybook/react-vite'
import type { AgentInfo, ClaudeAgent } from '@/lib/agents'
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

const claude: ClaudeAgent = {
  terminal: 'story-agent',
  sessionId: '0f3c2b1a-aaaa-bbbb-cccc-0123456789ab',
  name: 'dandelion-a6',
  agent: null,
  version: '2.1.330',
  status: 'waiting',
  waitingFor: 'permission prompt',
  updatedAt: 1795372085544,
  usage: {
    sessionId: '0f3c2b1a-aaaa-bbbb-cccc-0123456789ab',
    models: [
      {
        model: 'claude-opus-5-5',
        tokens: {
          inputTokens: 96,
          outputTokens: 39_430,
          cacheReadInputTokens: 3_233_619,
          cacheCreationInputTokens: 129_107,
        },
        costUsd: 0.42,
      },
    ],
    tokens: {
      inputTokens: 96,
      outputTokens: 39_430,
      cacheReadInputTokens: 3_233_619,
      cacheCreationInputTokens: 129_107,
    },
    costUsd: 0.42,
    unpricedModels: [],
    messages: 41,
  },
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
    claude,
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
