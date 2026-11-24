import { describe, expect, it } from 'vitest'
import {
  agentLabel,
  agentSocketUrl,
  blockedLabel,
  claudeFor,
  claudeLabel,
  claudeSummary,
  claudeUiStatus,
  formatTokens,
  uiStatus,
  unmanagedSessions,
  type AgentInfo,
  type ClaudeAgent,
} from './agents'

const agent: AgentInfo = {
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
}

describe('agents helpers', () => {
  it('maps backend states onto the five UI states', () => {
    expect(uiStatus(agent)).toBe('waiting')
    expect(uiStatus({ ...agent, agent_state: { state: 'working' } })).toBe('working')
    expect(uiStatus({ ...agent, agent_state: { state: 'idle' } })).toBe('idle')
    expect(uiStatus({ ...agent, agent_state: { state: 'done' } })).toBe('done')
    expect(uiStatus({ ...agent, agent_state: { state: 'error' } })).toBe('error')
    expect(uiStatus({ ...agent, agent_state: null })).toBe('idle')
  })

  it('labels the blocked reason', () => {
    expect(blockedLabel(agent.agent_state)).toBe('needs permission')
    expect(blockedLabel({ state: 'blocked', blocked_reason: 'question' })).toBe('asks a question')
    expect(blockedLabel({ state: 'blocked', blocked_reason: 'auth' })).toBe('needs sign-in')
    expect(blockedLabel({ state: 'blocked', blocked_reason: 'other' })).toBe('blocked')
    expect(blockedLabel({ state: 'idle' })).toBeNull()
    expect(blockedLabel(null)).toBeNull()
  })

  it('names an agent by title, then id', () => {
    expect(agentLabel(agent)).toBe('claude in dandelion')
    expect(agentLabel({ ...agent, title: '' })).toBe('t7')
  })

  it('builds the pane socket url', () => {
    expect(agentSocketUrl('t7')).toBe('ws://localhost:3000/api/agents/t7/ws')
    expect(agentSocketUrl('w1:p2')).toBe('ws://localhost:3000/api/agents/w1%3Ap2/ws')
  })
})

const claude: ClaudeAgent = {
  terminal: 't7',
  sessionId: '0f3c2b1a-aaaa-bbbb-cccc-0123456789ab',
  cwd: '/home/dev/dandelion',
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
          inputTokens: 0,
          outputTokens: 0,
          cacheReadInputTokens: 0,
          cacheCreationInputTokens: 0,
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

describe('claude helpers', () => {
  it('finds the session behind a terminal', () => {
    expect(claudeFor([claude], 't7')).toEqual(claude)
    expect(claudeFor([claude], 't9')).toBeNull()
    expect(claudeFor([claude], null)).toBeNull()
    expect(claudeFor(undefined, 't7')).toBeNull()
  })

  it('lists the sessions outside herdr as unmanaged', () => {
    const outside: ClaudeAgent = {
      ...claude,
      terminal: null,
      name: null,
      sessionId: 'abcdefgh-0000-0000-0000-000000000000',
    }

    expect(unmanagedSessions([claude, outside])).toEqual([outside])
    expect(unmanagedSessions(undefined)).toEqual([])
    expect(claudeLabel(outside)).toBe('session abcdefgh')
    expect(claudeLabel(claude)).toBe('dandelion-a6')
  })

  it('maps a Claude session state onto the UI states', () => {
    expect(claudeUiStatus(claude)).toBe('waiting')
    expect(claudeUiStatus({ ...claude, status: 'busy' })).toBe('working')
    expect(claudeUiStatus({ ...claude, status: 'idle' })).toBe('idle')
    expect(claudeUiStatus({ ...claude, status: 'paused-for-coffee' })).toBe('idle')
  })

  it('compacts token counts', () => {
    expect(formatTokens(950)).toBe('950')
    expect(formatTokens(12_345)).toBe('12.3k')
    expect(formatTokens(3_402_252)).toBe('3.4M')
    expect(formatTokens(999_950)).toBe('1.0M')
  })

  it('summarizes a waiting session', () => {
    expect(claudeSummary(claude)).toEqual({
      sessionId: '0f3c2b1a-aaaa-bbbb-cccc-0123456789ab',
      model: 'claude-opus-5-5',
      tokens: '3.4M',
      cost: '~$0.42',
      waiting: 'a permission prompt',
    })
  })

  it('marks an unpriced or untouched session', () => {
    expect(
      claudeSummary({
        ...claude,
        waitingFor: null,
        usage: {
          ...claude.usage,
          costUsd: 0,
          unpricedModels: ['mystery'],
          models: [{ ...claude.usage.models[0]!, costUsd: null }],
        },
      }),
    ).toMatchObject({ waiting: null, cost: 'unpriced' })

    expect(
      claudeSummary({
        ...claude,
        usage: {
          ...claude.usage,
          unpricedModels: ['mystery'],
          models: [
            ...claude.usage.models,
            { ...claude.usage.models[0]!, model: 'mystery', costUsd: null },
          ],
        },
      }),
    ).toMatchObject({ cost: '~$0.42+' })

    expect(
      claudeSummary({
        ...claude,
        waitingFor: 'input needed',
        usage: { ...claude.usage, messages: 0 },
      }),
    ).toMatchObject({ cost: '—', waiting: 'input' })
  })
})
