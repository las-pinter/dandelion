import { render, screen } from '@testing-library/react'
import { Panel } from './Panel'

describe('Panel', () => {
  it('renders the title, description, actions, and children', () => {
    render(
      <Panel title="Agents" description="Five tracked" actions={<button>Refresh</button>}>
        <p>agent list</p>
      </Panel>,
    )

    expect(screen.getByRole('heading', { name: 'Agents' })).toBeInTheDocument()
    expect(screen.getByText('Five tracked')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Refresh' })).toBeInTheDocument()
    expect(screen.getByText('agent list')).toBeInTheDocument()
  })

  it('tints the border and shows the label when waiting', () => {
    const { container } = render(
      <Panel title="Needs you" status="waiting">
        <p>question</p>
      </Panel>,
    )

    expect(screen.getByText('Waiting')).toBeInTheDocument()
    expect(container.firstElementChild).toHaveClass('border-status-waiting/40')
  })

  it('renders content without a header', () => {
    render(
      <Panel>
        <p>bare</p>
      </Panel>,
    )

    expect(screen.queryByRole('heading')).not.toBeInTheDocument()
    expect(screen.getByText('bare')).toBeInTheDocument()
  })
})
