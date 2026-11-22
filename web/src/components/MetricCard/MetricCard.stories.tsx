import type { Meta, StoryObj } from '@storybook/react-vite'
import { MetricCard } from './MetricCard'

const meta: Meta<typeof MetricCard> = {
  title: 'Building blocks/MetricCard',
  component: MetricCard,
}
export default meta

export const WithSparkline: StoryObj<typeof MetricCard> = {
  args: {
    label: 'CPU',
    value: '42',
    unit: '%',
    sparkline: [28, 35, 31, 40, 38, 45, 42],
    hint: '8 cores · 3.4 GHz',
  },
}

export const Plain: StoryObj<typeof MetricCard> = {
  args: { label: 'Uptime', value: '12d 04h' },
}
