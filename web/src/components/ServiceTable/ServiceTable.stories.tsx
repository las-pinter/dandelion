import type { Meta, StoryObj } from '@storybook/react-vite'
import type { UnitInfo } from '@/lib/services'
import { ServiceTable } from './ServiceTable'

const units: UnitInfo[] = [
  {
    name: 'app.service',
    description: 'The application server',
    load_state: 'loaded',
    active_state: 'active',
    sub_state: 'running',
    scope: 'user',
  },
  {
    name: 'nginx.service',
    description: 'A high performance web server',
    load_state: 'loaded',
    active_state: 'active',
    sub_state: 'running',
    scope: 'system',
  },
  {
    name: 'backup.timer',
    description: 'Nightly backup',
    load_state: 'loaded',
    active_state: 'inactive',
    sub_state: 'dead',
    scope: 'system',
  },
]

const meta: Meta<typeof ServiceTable> = {
  title: 'Building blocks/ServiceTable',
  component: ServiceTable,
}
export default meta

export const Default: StoryObj<typeof ServiceTable> = {
  args: {
    units,
    rules: [{ unit: 'nginx.service', verbs: ['restart'] }],
    selected: null,
    onSelect: () => undefined,
    onAction: () => undefined,
  },
}

export const NoAllowlist: StoryObj<typeof ServiceTable> = {
  args: {
    units,
    rules: [],
    selected: null,
    onSelect: () => undefined,
    onAction: () => undefined,
  },
}
