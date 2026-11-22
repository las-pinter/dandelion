import type { Meta, StoryObj } from '@storybook/react-vite'
import { AGENT_STATUSES } from '@/components/status'
import { StatusDot } from './StatusDot'

const meta: Meta<typeof StatusDot> = {
  title: 'Building blocks/StatusDot',
  component: StatusDot,
}
export default meta

export const Default: StoryObj<typeof StatusDot> = {
  args: { status: 'working' },
}

export const AllStates: StoryObj<typeof StatusDot> = {
  render: () => (
    <div className="flex flex-wrap gap-6 p-4">
      {AGENT_STATUSES.map((status) => (
        <StatusDot key={status} status={status} withLabel />
      ))}
    </div>
  ),
}
