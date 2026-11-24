import type { Meta, StoryObj } from '@storybook/react-vite'
import { XtermPane } from './XtermPane'

const meta: Meta<typeof XtermPane> = {
  title: 'Building blocks/XtermPane',
  component: XtermPane,
}
export default meta

/** The socket needs a running server, so Storybook shows a closed connection. */
export const Default: StoryObj<typeof XtermPane> = {
  args: {
    url: 'ws://localhost:6006/api/terminals/story/ws',
  },
}
