import { describe, expect, it } from 'vitest'
import { agentLabel, agentSocketUrl, blockedLabel, uiStatus, type AgentInfo } from './agents'

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
