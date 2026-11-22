import { render, screen } from '@testing-library/react'
import { Activity } from 'lucide-react'
import { createMemoryRouter, RouterProvider } from 'react-router'
import type { PageDefinition } from './page-registry'
import { buildRoutes } from './router'

const onePage: PageDefinition[] = [
  { path: '/one', title: 'One', icon: Activity, order: 0, Component: () => <p>page one</p> },
]

function renderAt(url: string, registered: PageDefinition[]) {
  render(
    <RouterProvider
      router={createMemoryRouter(buildRoutes(registered), { initialEntries: [url] })}
    />,
  )
}

describe('buildRoutes', () => {
  it('redirects the root to the first page', async () => {
    renderAt('/', onePage)

    expect(await screen.findByText('page one')).toBeInTheDocument()
  })

  it('renders a page for its nested path', () => {
    renderAt('/one/detail', onePage)

    expect(screen.getByText('page one')).toBeInTheDocument()
  })

  it('shows the not-found screen for an unknown path', () => {
    renderAt('/nope', onePage)

    expect(screen.getByRole('heading', { name: 'Page not found' })).toBeInTheDocument()
  })

  it('shows the not-found screen at the root when no pages exist', () => {
    renderAt('/', [])

    expect(screen.getByRole('heading', { name: 'Page not found' })).toBeInTheDocument()
  })
})
