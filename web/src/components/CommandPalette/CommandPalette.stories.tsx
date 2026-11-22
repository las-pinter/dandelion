import type { Meta, StoryObj } from '@storybook/react-vite'
import { Activity, Bot, TerminalSquare } from 'lucide-react'
import { useState } from 'react'
import { MemoryRouter } from 'react-router'
import type { PageDefinition } from '@/app/page-registry'
import { CommandPalette } from './CommandPalette'

const pages: PageDefinition[] = [
  { path: '/dashboard', title: 'Dashboard', icon: Activity, order: 0, Component: () => null },
  { path: '/agents', title: 'Agents', icon: Bot, order: 1, Component: () => null },
  { path: '/terminal', title: 'Terminal', icon: TerminalSquare, order: 2, Component: () => null },
]

function PaletteHarness() {
  const [open, setOpen] = useState(true)
  return (
    <MemoryRouter initialEntries={['/dashboard']}>
      <CommandPalette pages={pages} open={open} onOpenChange={setOpen} />
      <button type="button" onClick={() => setOpen(true)} className="m-4 rounded border px-3 py-2">
        Open palette
      </button>
    </MemoryRouter>
  )
}

const meta: Meta<typeof PaletteHarness> = {
  title: 'Overlays/CommandPalette',
  component: PaletteHarness,
}
export default meta

export const Open: StoryObj<typeof PaletteHarness> = { render: () => <PaletteHarness /> }
