import { render, screen } from '@testing-library/react'
import { Activity, Bot } from 'lucide-react'
import { createMemoryRouter, RouterProvider } from 'react-router'
import type { PageDefinition } from '@/app/page-registry'
import { AppShell } from './AppShell'

const pages: PageDefinition[] = [
  { path: '/one', title: 'One', icon: Activity, order: 1, Component: () => <p>page one</p> },
  { path: '/two', title: 'Two', icon: Bot, order: 2, Component: () => <p>page two</p> },
]

function renderAt(url: string) {
  const router = createMemoryRouter(
    [
      {
        element: <AppShell pages={pages} />,
        children: pages.map(({ path, Component }) => ({ path, element: <Component /> })),
      },
    ],
    { initialEntries: [url] },
  )
  render(<RouterProvider router={router} />)
}

describe('AppShell', () => {
  it('renders one navigation link per page', () => {
    renderAt('/one')

    const links = screen.getAllByRole('link').map((a) => a.textContent)

    expect(links).toEqual(['One', 'Two'])
  })

  it('renders the active page and marks its link current', () => {
    renderAt('/two')

    expect(screen.getByText('page two')).toBeInTheDocument()
    expect(screen.getByRole('link', { name: 'Two' })).toHaveAttribute('aria-current', 'page')
  })
})
