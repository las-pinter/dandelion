import type { Meta, StoryObj } from '@storybook/react-vite'
import type { ContainerInfo } from '@/lib/containers'
import { ContainerTable } from './ContainerTable'

const containers: ContainerInfo[] = [
  {
    id: 'aaa111bbb222ccc333',
    name: 'web',
    image: 'nginx:1.27',
    command: 'nginx -g daemon off;',
    state: 'running',
    status: 'Up 2 hours',
    created_at_unix: 1_700_000_000,
    labels: {},
  },
  {
    id: 'ddd444eee555fff666',
    name: 'db',
    image: 'postgres:17',
    command: null,
    state: 'exited',
    status: 'Exited (0) 3 hours ago',
    created_at_unix: 1_700_000_000,
    labels: {},
  },
  {
    id: 'ggg777hhh888iii999',
    name: 'worker',
    image: 'ghcr.io/example/worker:dev',
    command: 'node worker.js',
    state: 'paused',
    status: 'Up 20 minutes (Paused)',
    created_at_unix: 1_700_000_000,
    labels: {},
  },
]

const meta: Meta<typeof ContainerTable> = {
  title: 'Building blocks/ContainerTable',
  component: ContainerTable,
}
export default meta

export const Default: StoryObj<typeof ContainerTable> = {
  args: {
    containers,
    selected: null,
    onSelect: () => undefined,
    onAction: () => undefined,
  },
}

export const Selected: StoryObj<typeof ContainerTable> = {
  args: {
    containers,
    selected: containers[0],
    busy: 'web:restart',
    onSelect: () => undefined,
    onAction: () => undefined,
  },
}
