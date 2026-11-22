import { render, screen } from '@testing-library/react'
import { AGENT_STATUSES, STATUS_META } from '@/components/status'
import { StatusDot } from './StatusDot'

describe('StatusDot', () => {
  it.each(AGENT_STATUSES)('names the %s state for screen readers', (status) => {
    render(<StatusDot status={status} />)

    expect(screen.getByText(STATUS_META[status].label)).toBeInTheDocument()
    expect(screen.getByText(STATUS_META[status].label)).toHaveClass('sr-only')
  })

  it('shows a visible label when asked', () => {
    render(<StatusDot status="waiting" withLabel />)

    const label = screen.getByText('Waiting')
    expect(label).toBeInTheDocument()
    expect(label).not.toHaveClass('sr-only')
  })
})
