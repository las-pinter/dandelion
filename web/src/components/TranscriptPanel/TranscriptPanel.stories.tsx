import type { Meta, StoryObj } from '@storybook/react-vite'
import type { TranscriptRecord } from '@/lib/transcript'
import { TranscriptView } from './TranscriptPanel'

const records: TranscriptRecord[] = [
  {
    role: 'user',
    at: '2026-11-25T15:00:00Z',
    blocks: [{ kind: 'text', text: 'Run the tests and fix what fails.' }],
  },
  {
    role: 'assistant',
    at: '2026-11-25T15:00:04Z',
    blocks: [
      { kind: 'thinking', text: 'Check the gate first, then the failing suite.' },
      { kind: 'text', text: 'Running **just ci** now.' },
      {
        kind: 'tool_use',
        id: 'toolu_1',
        name: 'Bash',
        summary: 'just ci',
        input: '{\n  "command": "just ci"\n}',
      },
    ],
  },
  {
    role: 'assistant',
    at: '2026-11-25T15:00:18Z',
    blocks: [
      {
        kind: 'tool_use',
        id: 'toolu_2',
        name: 'Read',
        summary: 'web/src/components/AgentChat/AgentChat.tsx',
        input: '{\n  "file_path": "web/src/components/AgentChat/AgentChat.tsx"\n}',
      },
    ],
  },
  {
    role: 'user',
    at: '2026-11-25T15:00:31Z',
    blocks: [
      {
        kind: 'tool_result',
        toolUseId: 'toolu_1',
        isError: false,
        text: '463 tests passed',
      },
    ],
  },
  {
    role: 'assistant',
    at: '2026-11-25T15:00:35Z',
    blocks: [
      {
        kind: 'tool_use',
        id: 'toolu_x',
        name: 'Task',
        summary: 'Explore the agent socket',
        input: '{\n  "description": "Explore the agent socket"\n}',
      },
    ],
  },
  {
    role: 'system',
    at: '2026-11-25T15:01:00Z',
    blocks: [{ kind: 'divider', label: 'Conversation compacted' }],
  },
]

const meta = {
  title: 'Components/TranscriptPanel',
  component: TranscriptView,
  parameters: { layout: 'padded' },
  args: {
    sessionId: 'story-session',
    records,
    subagents: [
      {
        agent: 'agent-a1',
        toolUseId: 'toolu_x',
        description: 'Explore the agent socket',
      },
    ],
    hasEarlier: true,
    loading: false,
    error: null,
    onLoadEarlier: () => undefined,
  },
} satisfies Meta<typeof TranscriptView>

export default meta
type Story = StoryObj<typeof meta>

export const Conversation: Story = {}

export const Empty: Story = {
  args: { records: [], hasEarlier: false },
}

export const Failed: Story = {
  args: { records: [], loading: false, error: 'cannot read the transcript' },
}
