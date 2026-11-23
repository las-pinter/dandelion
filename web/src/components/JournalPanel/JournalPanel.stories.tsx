import type { Meta, StoryObj } from '@storybook/react-vite'
import type { JournalEntry, UnitInfo } from '@/lib/services'
import { JournalPanel } from './JournalPanel'

const unit: UnitInfo = {
  name: 'nginx.service',
  description: 'A high performance web server',
  load_state: 'loaded',
  active_state: 'active',
  sub_state: 'running',
  scope: 'system',
}

const entries: JournalEntry[] = [
  {
    timestamp_usec: 1_700_000_000_000_000,
    message: 'Started A high performance web server.',
    priority: 6,
    unit: 'nginx.service',
    identifier: 'systemd',
    pid: 1,
  },
  {
    timestamp_usec: 1_700_000_001_000_000,
    message: 'nginx: configuration file /etc/nginx/nginx.conf test is successful',
    priority: 3,
    unit: 'nginx.service',
    identifier: 'nginx',
    pid: 42,
  },
]

const meta: Meta<typeof JournalPanel> = {
  title: 'Building blocks/JournalPanel',
  component: JournalPanel,
}
export default meta

export const Default: StoryObj<typeof JournalPanel> = {
  args: { unit, entries, onRefresh: () => undefined },
}

export const Empty: StoryObj<typeof JournalPanel> = {
  args: { unit: null, entries: [], onRefresh: () => undefined },
}
