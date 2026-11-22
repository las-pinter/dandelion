import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { Activity, Bot } from 'lucide-react'
import { useState } from 'react'
import { MemoryRouter, Route, Routes } from 'react-router'
import type { PageDefinition } from '@/app/page-registry'
import { CommandPalette } from './CommandPalette'

const ping = vi.fn<() => void>()

const pages: PageDefinition[] = [
  { path: '/one', title: 'One', icon: Activity, order: 0, Component: () => null },
  {
    path: '/two',
    title: 'Two',
    icon: Bot,
    order: 1,
    Component: () => null,
    commands: [{ id: 'two:ping', label: 'Ping Two', group: 'Actions', run: ping }],
  },
]

function Harness() {
  const [open, setOpen] = useState(false)
  return (
    <>
      <CommandPalette pages={pages} open={open} onOpenChange={setOpen} />
      <Routes>
        <Route path="/one" element={<p>one page</p>} />
        <Route path="/two" element={<p>two page</p>} />
      </Routes>
    </>
  )
}

function renderPalette() {
  const user = userEvent.setup()
  render(
    <MemoryRouter initialEntries={['/one']}>
      <Harness />
    </MemoryRouter>,
  )
  return user
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
  ping.mockClear()
})

describe('CommandPalette', () => {
  it('opens with Ctrl+K', async () => {
    const user = renderPalette()

    await user.keyboard('{Control>}k{/Control}')

    expect(await screen.findByRole('dialog')).toBeInTheDocument()
    expect(screen.getByPlaceholderText('Search pages and actions…')).toBeInTheDocument()
  })

  it('opens with Meta+K', async () => {
    const user = renderPalette()

    await user.keyboard('{Meta>}k{/Meta}')

    expect(await screen.findByRole('dialog')).toBeInTheDocument()
  })

  it('lists every page and every contributed command', async () => {
    const user = renderPalette()
    await user.keyboard('{Control>}k{/Control}')

    await screen.findByRole('dialog')
    expect(screen.getByText('Go to One')).toBeInTheDocument()
    expect(screen.getByText('Go to Two')).toBeInTheDocument()
    expect(screen.getByText('Ping Two')).toBeInTheDocument()
  })

  it('navigates to the chosen page and closes', async () => {
    const user = renderPalette()
    await user.keyboard('{Control>}k{/Control}')
    await screen.findByRole('dialog')

    await user.click(screen.getByText('Go to Two'))

    expect(await screen.findByText('two page')).toBeInTheDocument()
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
  })

  it('runs a command contributed by a page', async () => {
    const user = renderPalette()
    await user.keyboard('{Control>}k{/Control}')
    await screen.findByRole('dialog')

    await user.click(screen.getByText('Ping Two'))

    expect(ping).toHaveBeenCalledTimes(1)
  })
})
