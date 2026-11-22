import type { Meta, StoryObj } from '@storybook/react-vite'
import { Activity, Bot, TerminalSquare } from 'lucide-react'
import { createMemoryRouter, RouterProvider } from 'react-router'
import type { PageDefinition } from '@/app/page-registry'
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

function ShellAt({ url }: { url: string }) {
  const router = createMemoryRouter(
    [
      {
        element: <AppShell pages={pages} />,
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
