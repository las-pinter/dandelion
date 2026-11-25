import { describe, expect, it } from 'vitest'
import {
  parseFrame,
  safeImageSource,
  subagentFor,
  transcriptSocketUrl,
  withAppended,
  withEarlier,
  type SubagentInfo,
  type TranscriptRecord,
} from './transcript'

const user: TranscriptRecord = {
  role: 'user',
  blocks: [{ kind: 'text', text: 'one' }],
}
const assistant: TranscriptRecord = {
  role: 'assistant',
  blocks: [{ kind: 'text', text: 'two' }],
}

describe('transcriptSocketUrl', () => {
  it('addresses the session socket on the current origin', () => {
    expect(transcriptSocketUrl('session 1')).toBe(
      `ws://${window.location.host}/api/claude/session%201/transcript/ws`,
    )
  })

  it('adds the subagent as a query parameter', () => {
    expect(transcriptSocketUrl('s', 'agent-a1')).toContain('?agent=agent-a1')
  })
})

describe('parseFrame', () => {
  it('reads a JSON frame', () => {
    const frame = parseFrame('{"type":"appended","records":[]}')

    expect(frame).toEqual({ type: 'appended', records: [] })
  })

  it('rejects malformed or untyped frames', () => {
    expect(parseFrame('nope')).toBeNull()
    expect(parseFrame({ type: 'page' })).toBeNull()
    expect(parseFrame('{"records":[]}')).toBeNull()
  })
})

describe('record lists', () => {
  it('prepends an earlier page and appends streamed records', () => {
    expect(withEarlier([assistant], [user])).toEqual([user, assistant])
    expect(withAppended([user], [assistant])).toEqual([user, assistant])
    const list = [user]
    expect(withAppended(list, [])).toBe(list)
  })

  it('finds the subagent a tool call spawned', () => {
    const items: SubagentInfo[] = [{ agent: 'agent-a1', toolUseId: 'toolu_x', description: null }]

    expect(subagentFor(items, 'toolu_x')?.agent).toBe('agent-a1')
    expect(subagentFor(items, 'toolu_y')).toBeUndefined()
  })
})

describe('safeImageSource', () => {
  it('allows only https images', () => {
    expect(safeImageSource('https://example.test/a.png')).toBe('https://example.test/a.png')
    expect(safeImageSource('http://example.test/a.png')).toBeNull()
    expect(safeImageSource(undefined)).toBeNull()
  })
})
