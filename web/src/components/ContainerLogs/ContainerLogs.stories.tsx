import type { Meta, StoryObj } from '@storybook/react-vite'
import type { ContainerInfo, ContainerLogLine } from '@/lib/containers'
import { ContainerLogs } from './ContainerLogs'

const web: ContainerInfo = {
  id: 'aaa111bbb222ccc333',
  name: 'web',
  image: 'nginx:1.27',
  command: 'nginx -g daemon off;',
  state: 'running',
  status: 'Up 2 hours',
  created_at_unix: 1_700_000_000,
  labels: {},
}

const lines: ContainerLogLine[] = [
  { stream: 'stdout', message: '2026-11-23T10:00:01.123456789Z starting nginx' },
  { stream: 'stdout', message: '2026-11-23T10:00:01.456789012Z listening on :80' },
  { stream: 'stderr', message: '2026-11-23T10:04:12.000000000Z upstream timed out' },
  { stream: 'stdout', message: 'plain line without a timestamp' },
]

const meta: Meta<typeof ContainerLogs> = {
  title: 'Building blocks/ContainerLogs',
  component: ContainerLogs,
}
export default meta

export const Default: StoryObj<typeof ContainerLogs> = {
  args: {
    container: web,
    lines,
    onRefresh: () => undefined,
  },
}

export const Empty: StoryObj<typeof ContainerLogs> = {
  args: {
    container: web,
    lines: [],
    onRefresh: () => undefined,
  },
}

export const NoSelection: StoryObj<typeof ContainerLogs> = {
  args: {
    container: null,
    lines: [],
    onRefresh: () => undefined,
  },
}
