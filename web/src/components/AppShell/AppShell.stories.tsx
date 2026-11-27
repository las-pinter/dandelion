import type { Meta, StoryObj } from '@storybook/react-vite'
import { Activity, Bot, TerminalSquare } from 'lucide-react'
import { createMemoryRouter, RouterProvider } from 'react-router'
import type { PageDefinition } from '@/app/page-registry'
import type { LiveSampleState } from '@/hooks/use-live-sample'
import { AppShell } from './AppShell'

const pages: PageDefinition[] = [
  {
    path: '/dashboard',
    title: 'Dashboard',
    icon: Activity,
    order: 0,
    Component: () => <p>Dashboard</p>,
  },
  {
    path: '/agents',
    title: 'Agents',
    icon: Bot,
    order: 1,
    badge: '1',
    Component: () => <p>Agents</p>,
  },
  {
    path: '/terminal',
    title: 'Terminal',
    icon: TerminalSquare,
    order: 2,
    Component: () => <p>Terminal</p>,
  },
]

/** A fixed reading: the story has no server, so the card cannot stream. */
const host: LiveSampleState = {
  connected: true,
  snapshot: {
    timestamp_ms: 1_795_372_085_544,
    uptime_secs: 94_000,
    cpu: {
      usage_percent: 12,
      per_core_percent: Array.from({ length: 10 }, () => 12),
      load: { one: 0.4, five: 0.5, fifteen: 0.6 },
    },
    memory: {
      total_bytes: 12 * 1024 ** 3,
      used_bytes: 3.3 * 1024 ** 3,
      available_bytes: 8 * 1024 ** 3,
      cache_bytes: 0,
      swap_total_bytes: 0,
      swap_used_bytes: 0,
    },
    disks: [],
    network: [],
    temperatures: [],
    top_processes_by_cpu: [],
    top_processes_by_memory: [],
  },
}

function ShellAt({ url }: { url: string }) {
  const router = createMemoryRouter(
    [
      {
        element: <AppShell pages={pages} host={host} />,
        children: pages.map(({ path, Component }) => ({ path, element: <Component /> })),
      },
    ],
    { initialEntries: [url] },
  )
  return <RouterProvider router={router} />
}

const meta: Meta<typeof ShellAt> = {
  title: 'Layout/AppShell',
  component: ShellAt,
}
export default meta

export const OnDashboard: StoryObj<typeof ShellAt> = { args: { url: '/dashboard' } }
export const OnAgents: StoryObj<typeof ShellAt> = { args: { url: '/agents' } }
