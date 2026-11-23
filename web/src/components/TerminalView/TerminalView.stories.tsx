import type { Meta, StoryObj } from '@storybook/react-vite'
import { TerminalView } from './TerminalView'

const meta: Meta<typeof TerminalView> = {
  title: 'Building blocks/TerminalView',
  component: TerminalView,
}
export default meta

/** The socket only answers from a running server, so Storybook shows a closed connection. */
export const Closed: StoryObj<typeof TerminalView> = {
  args: {
    id: 'story-terminal',
  },
}
