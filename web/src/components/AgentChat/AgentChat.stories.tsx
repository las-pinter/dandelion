import type { Meta, StoryObj } from '@storybook/react-vite'
import type { AgentInfo, ClaudeAgent } from '@/lib/agents'
import { AgentChat } from './AgentChat'

const agent: AgentInfo = {
  id: 'story-agent',
  title: 'claude in dandelion',
  cwd: '/home/dev/dandelion',
  labels: {},
  agent_kind: 'claude-code',
  agent_state: { state: 'working', blocked_reason: null, progress: null },
  lifecycle: 'live',
  created_ms: 1_700_000_000_000,
  exit_code: null,
  finished_reason: null,
}

const claude: ClaudeAgent = {
  terminal: 'story-agent',
  sessionId: '0f3c2b1a-aaaa-bbbb-cccc-0123456789ab',
  cwd: '/home/dev/dandelion',
  name: 'dandelion-a6',
  agent: null,
  version: '2.1.330',
  status: 'busy',
  waitingFor: null,
  updatedAt: 1795372085544,
  usage: {
    sessionId: '0f3c2b1a-aaaa-bbbb-cccc-0123456789ab',
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
}

const meta: Meta<typeof AgentChat> = {
  title: 'Building blocks/AgentChat',
  component: AgentChat,
}
export default meta

/** The sockets need a running server, so Storybook shows closed connections. */
export const Working: StoryObj<typeof AgentChat> = {
  args: { agent, claude, onAct: () => undefined },
}

/** An agent with no Claude session behind it: the terminal is the whole view. */
export const WithoutTranscript: StoryObj<typeof AgentChat> = {
  args: {
    agent: { ...agent, agent_state: { state: 'idle', blocked_reason: null, progress: null } },
    claude: null,
    onAct: () => undefined,
  },
}

/** A question block swaps the prompt for the answer bar. */
export const Question: StoryObj<typeof AgentChat> = {
  args: {
    agent: {
      ...agent,
      agent_state: { state: 'blocked', blocked_reason: 'question', progress: null },
    },
    claude,
    onAct: () => undefined,
  },
}
