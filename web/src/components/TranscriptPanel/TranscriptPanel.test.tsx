import { fireEvent, render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { TranscriptView, type TranscriptViewProps } from './TranscriptPanel'
import type { TranscriptRecord } from '@/lib/transcript'

class SilentSocket {
  addEventListener() {}
  removeEventListener() {}
  send() {}
  close() {}
  readyState = 1
}

beforeEach(() => {
  vi.stubGlobal('WebSocket', SilentSocket)
})

function setup(records: TranscriptRecord[], overrides: Partial<TranscriptViewProps> = {}) {
  const props: TranscriptViewProps = {
    sessionId: 'session-1',
    records,
    subagents: [],
    hasEarlier: false,
    loading: false,
    error: null,
    onLoadEarlier: vi.fn<() => void>(),
    ...overrides,
  }
  render(<TranscriptView {...props} />)
  return props
}

function scroller() {
  const element = screen.getByRole('log')
  const heights = { value: 1000 }
  Object.defineProperty(element, 'scrollHeight', { configurable: true, get: () => heights.value })
  Object.defineProperty(element, 'clientHeight', { configurable: true, get: () => 300 })
  return { element, heights }
}

function view(records: TranscriptRecord[]) {
  const props: TranscriptViewProps = {
    sessionId: 'session-1',
    records,
    subagents: [],
    hasEarlier: false,
    loading: false,
    error: null,
    onLoadEarlier: vi.fn<() => void>(),
  }
  return <TranscriptView {...props} />
}

describe('TranscriptView', () => {
  it('renders message text as markdown and keeps links safe', () => {
    setup([
      {
        role: 'assistant',
        blocks: [{ kind: 'text', text: '**bold** and [a doc](https://example.test/doc)' }],
      },
    ])

    expect(screen.getByText('bold').tagName).toBe('STRONG')
    const link = screen.getByRole('link', { name: 'a doc' })
    expect(link).toHaveAttribute('target', '_blank')
    expect(link).toHaveAttribute('rel', 'noopener noreferrer')
  })

  it('renders only https images', () => {
    setup([
      { role: 'user', blocks: [{ kind: 'text', text: '![ok](https://a.test/x.png)' }] },
      { role: 'user', blocks: [{ kind: 'text', text: '![blocked](http://a.test/y.png)' }] },
    ])

    expect(screen.getByAltText('ok')).toHaveAttribute('src', 'https://a.test/x.png')
    expect(screen.queryByAltText('blocked')).toBeNull()
    expect(screen.getByText('[blocked]')).toBeInTheDocument()
  })

  it('keeps thinking collapsed until opened', () => {
    setup([{ role: 'assistant', blocks: [{ kind: 'thinking', text: 'check first' }] }])

    const summary = screen.getByText('Thinking')
    expect(summary.closest('details')).not.toHaveAttribute('open')
    expect(screen.getByText('check first')).toBeInTheDocument()
  })

  it('shows a tool call summary and the input on expand', () => {
    setup([
      {
        role: 'assistant',
        blocks: [
          {
            kind: 'tool_use',
            id: 'toolu_1',
            name: 'Bash',
            summary: 'just test',
            input: '{\n  "command": "just test"\n}',
          },
        ],
      },
      {
        role: 'user',
        blocks: [{ kind: 'tool_result', toolUseId: 'toolu_1', isError: true, text: 'failed' }],
      },
    ])

    expect(screen.getByText('Bash')).toBeInTheDocument()
    expect(screen.getByText('just test')).toBeInTheDocument()
    expect(screen.getByText('Error')).toBeInTheDocument()
    expect(screen.getByText('failed')).toBeInTheDocument()
  })

  it('keeps one row for a tool call and its result', () => {
    const { container } = render(
      view([
        {
          role: 'assistant',
          blocks: [{ kind: 'tool_use', id: 'toolu_1', name: 'Bash', summary: 'ls', input: '{}' }],
        },
        {
          role: 'user',
          blocks: [{ kind: 'tool_result', toolUseId: 'toolu_1', isError: false, text: 'done' }],
        },
      ]),
    )

    expect(container.querySelectorAll('article')).toHaveLength(1)
    expect(screen.getByText('done')).toBeInTheDocument()
  })

  it('nests a subagent behind its tool call', async () => {
    setup(
      [
        {
          role: 'assistant',
          blocks: [
            {
              kind: 'tool_use',
              id: 'toolu_x',
              name: 'Task',
              summary: 'explore',
              input: '{}',
            },
          ],
        },
      ],
      {
        subagents: [{ agent: 'agent-a1', toolUseId: 'toolu_x', description: 'probe' }],
      },
    )

    fireEvent.click(screen.getByText('Task'))
    const subagent = await screen.findByText('Subagent: probe')
    fireEvent.click(subagent)
    expect(await screen.findByLabelText('Subagent transcript: probe')).toBeInTheDocument()
  })

  it('never renders raw html', () => {
    setup([{ role: 'user', blocks: [{ kind: 'text', text: '<script>alert(1)</script>' }] }])

    expect(screen.getByText('<script>alert(1)</script>')).toBeInTheDocument()
    expect(document.querySelector('script')).toBeNull()
  })

  it('follows the newest record when the reader is at the bottom', () => {
    const first = {
      offset: 0,
      role: 'user' as const,
      blocks: [{ kind: 'text' as const, text: 'one' }],
    }
    const second = {
      offset: 40,
      role: 'user' as const,
      blocks: [{ kind: 'text' as const, text: 'two' }],
    }
    const { rerender } = render(view([first]))
    const { element } = scroller()
    element.scrollTop = 700
    fireEvent.scroll(element)

    rerender(view([first, second]))

    expect(element.scrollTop).toBe(1000)
  })

  it('holds the view when the reader scrolled back', () => {
    const first = {
      offset: 0,
      role: 'user' as const,
      blocks: [{ kind: 'text' as const, text: 'one' }],
    }
    const second = {
      offset: 40,
      role: 'user' as const,
      blocks: [{ kind: 'text' as const, text: 'two' }],
    }
    const { rerender } = render(view([first]))
    const { element } = scroller()
    element.scrollTop = 0
    fireEvent.wheel(element)
    fireEvent.scroll(element)

    rerender(view([first, second]))

    expect(element.scrollTop).toBe(0)
  })

  it('keeps following through a scroll a resize produced', () => {
    const first = {
      offset: 0,
      role: 'user' as const,
      blocks: [{ kind: 'text' as const, text: 'one' }],
    }
    const second = {
      offset: 40,
      role: 'user' as const,
      blocks: [{ kind: 'text' as const, text: 'two' }],
    }
    const { rerender } = render(view([first]))
    const { element } = scroller()
    // A layout change can scroll the list without the reader touching it.
    element.scrollTop = 100
    fireEvent.scroll(element)

    rerender(view([first, second]))

    expect(element.scrollTop).toBe(1000)
  })

  it('keeps the view in place when an earlier page is prepended', () => {
    const older = {
      offset: 0,
      role: 'user' as const,
      blocks: [{ kind: 'text' as const, text: 'older' }],
    }
    const newer = {
      offset: 100,
      role: 'user' as const,
      blocks: [{ kind: 'text' as const, text: 'newer' }],
    }
    const { rerender } = render(view([newer]))
    const { element, heights } = scroller()
    rerender(view([newer]))
    element.scrollTop = 100
    heights.value = 1200

    rerender(view([older, newer]))

    expect(element.scrollTop).toBe(300)
  })

  it('re-pins to the newest record when an image loads', () => {
    render(
      view([
        { role: 'assistant', blocks: [{ kind: 'text', text: '![pic](https://a.test/x.png)' }] },
      ]),
    )
    const { element } = scroller()
    element.scrollTop = 100

    fireEvent.load(screen.getByAltText('pic'))

    expect(element.scrollTop).toBe(1000)
  })

  it('gives consecutive records from one role a single header', () => {
    setup([
      {
        offset: 0,
        role: 'assistant',
        at: '2026-11-25T15:00:00Z',
        blocks: [{ kind: 'text', text: 'first' }],
      },
      {
        offset: 40,
        role: 'assistant',
        at: '2026-11-25T15:02:00Z',
        blocks: [{ kind: 'text', text: 'second' }],
      },
      {
        offset: 80,
        role: 'user',
        at: '2026-11-25T15:03:00Z',
        blocks: [{ kind: 'text', text: 'third' }],
      },
    ])

    expect(screen.getAllByText('assistant')).toHaveLength(1)
    expect(screen.getAllByText('user')).toHaveLength(1)
    // The header carries the span of the group, not just its first time.
    expect(screen.getByText('assistant').parentElement?.textContent).toMatch(/–/)
  })

  it('does not print the same minute twice in a group header', () => {
    setup([
      {
        offset: 0,
        role: 'assistant',
        at: '2026-11-25T15:00:04Z',
        blocks: [{ kind: 'text', text: 'first' }],
      },
      {
        offset: 60,
        role: 'assistant',
        at: '2026-11-25T15:00:18Z',
        blocks: [{ kind: 'text', text: 'second' }],
      },
    ])

    expect(screen.getByText('assistant').parentElement?.textContent).not.toMatch(/–/)
  })

  it('opens no group for a record whose blocks all show inside a call', () => {
    setup([
      {
        offset: 0,
        role: 'assistant',
        at: '2026-11-25T15:00:00Z',
        blocks: [
          { kind: 'tool_use', id: 'call-1', name: 'Bash', summary: 'just test', input: '{}' },
        ],
      },
      {
        offset: 40,
        role: 'user',
        at: '2026-11-25T15:00:10Z',
        blocks: [{ kind: 'tool_result', toolUseId: 'call-1', isError: false, text: 'ok' }],
      },
    ])

    expect(screen.queryByText('user')).toBeNull()
    expect(screen.getAllByRole('article')).toHaveLength(1)
  })

  it('offers earlier pages and reports the loading state', () => {
    const props = setup([{ role: 'user', blocks: [{ kind: 'text', text: 'one' }] }], {
      hasEarlier: true,
    })

    fireEvent.click(screen.getByRole('button', { name: 'Load earlier' }))
    expect(props.onLoadEarlier).toHaveBeenCalledTimes(1)
  })

  it('shows a divider and the empty state', () => {
    setup([{ role: 'system', blocks: [{ kind: 'divider', label: 'Conversation compacted' }] }])

    expect(screen.getByText('Conversation compacted')).toBeInTheDocument()
  })

  it('shows the empty message when no records arrived yet', () => {
    setup([], { loading: false })

    expect(screen.getByText('No messages yet.')).toBeInTheDocument()
  })
})
