import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import type { TerminalInfo } from '@/lib/terminal'
import { TerminalTabs } from './TerminalTabs'

vi.mock('@/components/TerminalView/TerminalView', () => ({
  TerminalView: ({ id, active }: { id: string; active?: boolean }) => (
    <div data-testid="terminal-view" data-id={id} data-active={String(active)} />
  ),
}))

const terminals: TerminalInfo[] = [
  {
    id: 'aaa111',
    shell: '/bin/bash',
    cols: 80,
    rows: 24,
    created_at_unix: 1_700_000_000,
    attached: true,
  },
  {
    id: 'bbb222',
    shell: '/usr/bin/fish',
    cols: 80,
    rows: 24,
    created_at_unix: 1_700_000_100,
    attached: true,
  },
]

function setup(activeId: string | null = 'aaa111') {
  const onSelect = vi.fn<(id: string) => void>()
  const onClose = vi.fn<(id: string) => void>()
  render(
    <TerminalTabs
      terminals={terminals}
      activeId={activeId}
      onSelect={onSelect}
      onClose={onClose}
    />,
  )
  return { onSelect, onClose }
}

describe('TerminalTabs', () => {
  it('renders one tab and one view per shell', () => {
    setup()

    expect(screen.getByRole('tab', { name: 'Terminal 1: bash' })).toHaveAttribute(
      'aria-selected',
      'true',
    )
    expect(screen.getByRole('tab', { name: 'Terminal 2: fish' })).toHaveAttribute(
      'aria-selected',
      'false',
    )
    expect(screen.getAllByTestId('terminal-view')).toHaveLength(2)
  })

  it('activates only the selected view', () => {
    setup('bbb222')

    const views = screen.getAllByTestId('terminal-view')
    expect(views[0]).toHaveAttribute('data-active', 'false')
    expect(views[1]).toHaveAttribute('data-active', 'true')
  })

  it('selects and closes a tab', async () => {
    const user = userEvent.setup()
    const { onSelect, onClose } = setup()

    await user.click(screen.getByRole('tab', { name: 'Terminal 2: fish' }))
    expect(onSelect).toHaveBeenCalledWith('bbb222')

    await user.click(screen.getByRole('button', { name: 'Close Terminal 1: bash' }))
    expect(onClose).toHaveBeenCalledWith('aaa111')
  })
})
