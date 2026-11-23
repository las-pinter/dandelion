import type { Meta, StoryObj } from '@storybook/react-vite'
import { MetricChart } from './MetricChart'

const now = Date.now()
const points = Array.from({ length: 120 }, (_, index) => ({
  timestamp: now - (119 - index) * 30_000,
  value: 35 + Math.sin(index / 9) * 18 + (index % 7),
}))

const meta: Meta<typeof MetricChart> = {
  title: 'Building blocks/MetricChart',
  component: MetricChart,
}
export default meta

export const Cpu: StoryObj<typeof MetricChart> = {
  args: { label: 'CPU', unit: '%', points, domain: [0, 100], color: 'var(--chart-1)' },
}

export const Memory: StoryObj<typeof MetricChart> = {
  args: {
    label: 'Memory',
    unit: 'GiB',
    points: points.map((point) => ({ ...point, value: 6 + point.value / 20 })),
    color: 'var(--chart-2)',
  },
}

export const Waiting: StoryObj<typeof MetricChart> = {
  args: { label: 'Load', points: [] },
}
