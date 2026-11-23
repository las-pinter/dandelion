import { render, screen } from '@testing-library/react'
import { Activity } from 'lucide-react'
import { RouterProvider, createMemoryRouter } from 'react-router'
import { afterEach, beforeEach, vi } from 'vitest'
import type { PageDefinition } from './page-registry'
import { buildRoutes } from './router'

const onePage: PageDefinition[] = [
  { path: '/one', title: 'One', icon: Activity, order: 0, Component: () => <p>page one</p> },
]

function authResponse(body: { setup_required: boolean; authenticated: boolean }): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  })
}

function renderAt(url: string, registered: PageDefinition[]) {
  render(
    <RouterProvider
      router={createMemoryRouter(buildRoutes(registered), { initialEntries: [url] })}
    />,
  )
}

beforeEach(() => {
  vi.stubGlobal(
    'fetch',
    vi.fn<typeof fetch>(async () => authResponse({ setup_required: false, authenticated: true })),
  )
})

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('buildRoutes', () => {
  it('redirects the root to the first page', async () => {
    renderAt('/', onePage)

    expect(await screen.findByText('page one')).toBeInTheDocument()
  })

  it('renders a page for its nested path', async () => {
    renderAt('/one/detail', onePage)

    expect(await screen.findByText('page one')).toBeInTheDocument()
  })

  it('shows the not-found screen for an unknown path', async () => {
    renderAt('/nope', onePage)

    expect(await screen.findByRole('heading', { name: 'Page not found' })).toBeInTheDocument()
  })

  it('shows the not-found screen at the root when no pages exist', async () => {
    renderAt('/', [])

    expect(await screen.findByRole('heading', { name: 'Page not found' })).toBeInTheDocument()
  })

  it('renders a standalone page without the app shell', async () => {
    const registered: PageDefinition[] = [
      ...onePage,
      {
        path: '/login',
        title: 'Sign in',
        icon: Activity,
        order: 900,
        standalone: true,
        Component: () => <p>sign in here</p>,
      },
    ]

    renderAt('/login', registered)

    expect(await screen.findByText('sign in here')).toBeInTheDocument()
    expect(screen.queryByRole('navigation', { name: 'Primary' })).not.toBeInTheDocument()
  })
})
