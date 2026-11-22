import type { Meta, StoryObj } from '@storybook/react-vite'
import { Panel } from './Panel'

const meta: Meta<typeof Panel> = {
  title: 'Building blocks/Panel',
  component: Panel,
}
export default meta

export const Default: StoryObj<typeof Panel> = {
  args: {
    title: 'Agents',
    description: 'Five tracked on this host',
    status: 'waiting',
    children: <p className="text-sm text-muted-foreground">Agent rows live here.</p>,
  },
}

export const Bare: StoryObj<typeof Panel> = {
  args: { children: <p className="text-sm">Content without a header.</p> },
}
