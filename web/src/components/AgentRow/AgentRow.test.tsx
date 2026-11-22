import { render, screen } from '@testing-library/react'
import { AgentRow } from './AgentRow'

describe('AgentRow', () => {
  it('shows the name, command, detail, and status label', () => {
    render(<AgentRow name="herdr" command="herdr attach lane-9" status="working" detail="12m" />)

    expect(screen.getByText('herdr')).toBeInTheDocument()
    expect(screen.getByText('herdr attach lane-9')).toBeInTheDocument()
    expect(screen.getByText('12m')).toBeInTheDocument()
    expect(screen.getByText('Working')).toBeInTheDocument()
  })
})
