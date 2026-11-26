import { describe, expect, it } from 'vitest'
import {
  formatTime,
  groupRecords,
  parseFrame,
  roleOf,
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

describe('transcript groups', () => {
  const at = (minute: number) => `2026-11-25T15:${String(minute).padStart(2, '0')}:00Z`

  it('groups consecutive records from one role', () => {
    const groups = groupRecords(
      [
        { offset: 0, role: 'assistant', at: at(0), blocks: [{ kind: 'text', text: 'a' }] },
        { offset: 40, role: 'assistant', at: at(2), blocks: [{ kind: 'text', text: 'b' }] },
        { offset: 80, role: 'user', at: at(3), blocks: [{ kind: 'text', text: 'c' }] },
      ],
      new Set(),
    )

    expect(groups.map((group) => group.role)).toEqual(['assistant', 'user'])
    expect(groups[0]?.records).toHaveLength(2)
    expect(groups[0]?.at).toBe(at(0))
    expect(groups[0]?.end).toBe(at(2))
  })

  it('opens no group for a record whose blocks all show inside a call', () => {
    const groups = groupRecords(
      [
        {
          offset: 0,
          role: 'assistant',
          at: at(0),
          blocks: [{ kind: 'tool_use', id: 'call', name: 'Bash', summary: 'ls', input: '{}' }],
        },
        {
          offset: 40,
          role: 'user',
          at: at(1),
          blocks: [{ kind: 'tool_result', toolUseId: 'call', isError: false, text: 'ok' }],
        },
      ],
      new Set(['call']),
    )

    expect(groups).toHaveLength(1)
    expect(groups[0]?.role).toBe('assistant')
  })

  it('reads a role the server did not name as a system record', () => {
    expect(roleOf('assistant')).toBe('assistant')
    expect(roleOf('tool')).toBe('system')
    expect(roleOf(undefined)).toBe('system')
  })

  it('formats a time to the minute and leaves an unparsable value alone', () => {
    expect(formatTime('2026-11-25T15:04:09Z')).toMatch(/04/)
    expect(formatTime('not a time')).toBe('not a time')
  })
})
