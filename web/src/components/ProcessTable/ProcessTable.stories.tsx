import type { Meta, StoryObj } from '@storybook/react-vite'
import type { ProcessInfo } from '@/lib/processes'
import { ProcessTable } from './ProcessTable'

const processes: ProcessInfo[] = [
  {
    pid: 1,
    parent_pid: null,
    name: 'systemd',
    user: 'root',
    state: 'sleeping',
    cpu_seconds: 12.4,
    memory_bytes: 12 * 1024 ** 2,
    command: '/sbin/init splash',
    started_at_unix: 1_700_000_000,
    niceness: 0,
  },
  {
    pid: 812,
    parent_pid: 1,
    name: 'node',
    user: 'dev',
    state: 'running',
    cpu_seconds: 412.8,
    memory_bytes: 380 * 1024 ** 2,
    command: 'node /home/dev/dandelion/web/node_modules/.bin/vite',
    started_at_unix: 1_700_000_500,
    niceness: 0,
  },
  {
    pid: 1440,
    parent_pid: 812,
    name: 'rust-analyzer',
    user: 'dev',
    state: 'disk_sleep',
    cpu_seconds: 2_480.2,
    memory_bytes: 1_800 * 1024 ** 2,
    command: 'rust-analyzer --stdio',
    started_at_unix: 1_700_001_200,
    niceness: 5,
  },
]

const meta: Meta<typeof ProcessTable> = {
  title: 'Building blocks/ProcessTable',
  component: ProcessTable,
}
export default meta

export const Default: StoryObj<typeof ProcessTable> = {
  args: {
    processes,
    onSignal: () => undefined,
    onRenice: () => undefined,
  },
}

export const Busy: StoryObj<typeof ProcessTable> = {
  args: {
    processes,
    busyPid: 812,
    onSignal: () => undefined,
    onRenice: () => undefined,
  },
}
