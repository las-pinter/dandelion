import { render, screen } from '@testing-library/react'
import { AGENT_STATUSES, STATUS_META } from '@/components/status'
import { StatusPill } from './StatusPill'

describe('StatusPill', () => {
  it.each(AGENT_STATUSES)('shows the %s state as visible text', (status) => {
    render(<StatusPill status={status} />)

    const pill = screen.getByText(STATUS_META[status].label)
    expect(pill).toBeVisible()
    expect(pill.closest('[data-status]')).toHaveAttribute('data-status', status)
  })
})
