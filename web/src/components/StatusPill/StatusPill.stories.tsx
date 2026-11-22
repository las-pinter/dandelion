import type { Meta, StoryObj } from '@storybook/react-vite'
import { AGENT_STATUSES } from '@/components/status'
import { StatusPill } from './StatusPill'

const meta: Meta<typeof StatusPill> = {
  title: 'Building blocks/StatusPill',
  component: StatusPill,
}
export default meta

export const Default: StoryObj<typeof StatusPill> = {
  args: { status: 'waiting' },
}

export const AllStates: StoryObj<typeof StatusPill> = {
  render: () => (
    <div className="flex flex-wrap gap-2 p-4">
      {AGENT_STATUSES.map((status) => (
        <StatusPill key={status} status={status} />
      ))}
    </div>
  ),
}
