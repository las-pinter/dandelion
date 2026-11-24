import type { Meta, StoryObj } from '@storybook/react-vite'
import type { TerminalInfo } from '@/lib/terminal'
import { TerminalTabs } from './TerminalTabs'

const terminals: TerminalInfo[] = [
  {
    id: 'story-1',
    shell: '/bin/bash',
    cols: 80,
    rows: 24,
    created_at_unix: 1_700_000_000,
    attached: true,
  },
  {
    id: 'story-2',
    shell: '/usr/bin/fish',
    cols: 80,
    rows: 24,
    created_at_unix: 1_700_000_100,
    attached: true,
  },
]

const meta: Meta<typeof TerminalTabs> = {
  title: 'Building blocks/TerminalTabs',
  component: TerminalTabs,
}
export default meta

export const Default: StoryObj<typeof TerminalTabs> = {
  args: {
    terminals,
    activeId: terminals[0]?.id ?? null,
    onSelect: () => undefined,
    onClose: () => undefined,
  },
}
