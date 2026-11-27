import { render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { Activity, Bot } from 'lucide-react'
import { createMemoryRouter, RouterProvider } from 'react-router'
import { vi } from 'vitest'
import type { PageDefinition } from '@/app/page-registry'
import type { LiveSampleState } from '@/hooks/use-live-sample'
import { formatBytes } from '@/lib/metrics'
import { AppShell } from './AppShell'

// The host card reads live metrics; most tests need none.
const live = vi.hoisted(() => ({
  state: { snapshot: null, connected: false } as LiveSampleState,
}))
vi.mock('@/hooks/use-live-sample', () => ({ useLiveSample: () => live.state }))

/** A machine reading: 10 cores, 12 GiB total, 3.3 GiB used. */
function hostSample(): LiveSampleState {
  return {
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
}

const pages: PageDefinition[] = [
  { path: '/one', title: 'One', icon: Activity, order: 1, Component: () => <p>page one</p> },
  {
    path: '/two',
    title: 'Two',
    icon: Bot,
    order: 2,
    badge: '3',
    Component: () => <p>page two</p>,
  },
]

function renderAt(url: string, registered: PageDefinition[] = pages) {
  const router = createMemoryRouter(
    [
      {
        element: <AppShell pages={registered} />,
        children: [
          ...registered.map(({ path, Component }) => ({ path, element: <Component /> })),
          { path: '*', element: <h1>Page not found</h1> },
        ],
      },
    ],
    { initialEntries: [url] },
  )
  render(<RouterProvider router={router} />)
  return userEvent.setup()
}

class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}

beforeAll(() => {
  Element.prototype.scrollIntoView = vi.fn<() => void>()
  vi.stubGlobal('ResizeObserver', ResizeObserverStub)
})

afterAll(() => {
  vi.unstubAllGlobals()
})

beforeEach(() => {
  document.documentElement.classList.add('dark')
  live.state = { snapshot: null, connected: false }
})

afterEach(() => {
  document.documentElement.classList.remove('dark', 'light')
  delete document.documentElement.dataset.theme
  document.documentElement.style.colorScheme = ''
})

describe('AppShell', () => {
  it('renders one sidebar link per registered page', () => {
    renderAt('/one')

    const nav = screen.getByRole('navigation', { name: 'Primary' })

    expect(within(nav).getByRole('link', { name: 'One' })).toBeInTheDocument()
    expect(within(nav).getByRole('link', { name: /Two/ })).toBeInTheDocument()
  })

  it('shows the page title in the top bar and marks the link current', () => {
    renderAt('/two')

    expect(screen.getByRole('heading', { name: 'Two' })).toBeInTheDocument()
    expect(document.title).toBe('Two · Dandelion')
    const nav = screen.getByRole('navigation', { name: 'Primary' })
    expect(within(nav).getByRole('link', { name: /Two/ })).toHaveAttribute('aria-current', 'page')
  })

  it('shows the registered badge, with a readable accessible name, and the host card', () => {
    renderAt('/one')

    const nav = screen.getByRole('navigation', { name: 'Primary' })
    expect(within(nav).getByRole('link', { name: 'Two, 3 notifications' })).toBeInTheDocument()
    expect(screen.getByText('Host')).toBeInTheDocument()
    expect(screen.getByText('Reading the machine…')).toBeInTheDocument()
    expect(screen.getByText('Connecting…')).toBeInTheDocument()
  })

  it('shows the live cores, memory and stream state on the host card', () => {
    live.state = hostSample()
    renderAt('/one')

    expect(screen.getByText(`10 cores · ${formatBytes(12 * 1024 ** 3)}`)).toBeInTheDocument()
    expect(screen.getByText(`${formatBytes(3.3 * 1024 ** 3)} used`)).toBeInTheDocument()
    expect(screen.getByText('Live')).toBeInTheDocument()
  })

  it('offers a skip link to the page content', () => {
    renderAt('/one')

    const skip = screen.getByRole('link', { name: 'Skip to content' })

    expect(skip).toHaveAttribute('href', '#main')
    expect(document.getElementById('main')).toBeInTheDocument()
  })

  it('leaves the only h1 to the body when no page owns the route', () => {
    renderAt('/missing')

    const headings = screen.getAllByRole('heading', { level: 1 })

    expect(headings.map((heading) => heading.textContent)).toEqual(['Page not found'])
  })

  it('puts pages past the fourth into a More sheet', async () => {
    const manyPages: PageDefinition[] = Array.from({ length: 5 }, (_, index) => {
      const number = index + 1
      return {
        path: `/page-${number}`,
        title: `Page ${number}`,
        icon: Activity,
        order: index,
        Component: () => <p>page {number}</p>,
      }
    })
    const user = renderAt('/page-1', manyPages)

    const mobile = screen.getByRole('navigation', { name: 'Primary mobile' })
    expect(within(mobile).getByRole('link', { name: 'Page 4' })).toBeInTheDocument()
    expect(within(mobile).queryByRole('link', { name: 'Page 5' })).not.toBeInTheDocument()

    await user.click(within(mobile).getByRole('button', { name: 'More' }))
    const sheet = await screen.findByRole('dialog')
    await user.click(within(sheet).getByRole('link', { name: 'Page 5' }))

    expect(await screen.findByText('page 5')).toBeInTheDocument()
  })

  it('gives phones a bottom navigation bar', async () => {
    const user = renderAt('/one')

    const mobile = screen.getByRole('navigation', { name: 'Primary mobile' })
    expect(within(mobile).getByRole('link', { name: 'One' })).toBeInTheDocument()

    await user.click(within(mobile).getByRole('link', { name: /Two/ }))

    expect(await screen.findByText('page two')).toBeInTheDocument()
    expect(screen.getByRole('heading', { name: 'Two' })).toBeInTheDocument()
  })

  it('opens the command palette from the top bar', async () => {
    const user = renderAt('/one')

    await user.click(screen.getByRole('button', { name: 'Open command palette' }))

    expect(await screen.findByRole('dialog')).toBeInTheDocument()
    expect(screen.getByText('Go to Two')).toBeInTheDocument()
  })
})
