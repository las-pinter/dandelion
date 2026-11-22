import type { Meta, StoryObj } from '@storybook/react-vite'
import { AgentRow } from './AgentRow'

const meta: Meta<typeof AgentRow> = {
  title: 'Building blocks/AgentRow',
  component: AgentRow,
}
export default meta

export const Default: StoryObj<typeof AgentRow> = {
  args: {
    name: 'herdr',
    command: 'herdr attach lane-9',
    status: 'working',
    detail: '12m',
  },
}
