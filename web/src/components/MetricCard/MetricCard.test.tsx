import { render, screen } from '@testing-library/react'
import { MetricCard } from './MetricCard'

describe('MetricCard', () => {
  it('shows the label, value, unit, and hint', () => {
    render(<MetricCard label="Memory" value="15.2" unit="GiB" hint="of 32 GiB" />)

    expect(screen.getByText('Memory')).toBeInTheDocument()
    expect(screen.getByText('15.2')).toBeInTheDocument()
    expect(screen.getByText('GiB')).toBeInTheDocument()
    expect(screen.getByText('of 32 GiB')).toBeInTheDocument()
  })

  it('draws one sparkline with an accessible name', () => {
    const { container } = render(
      <MetricCard label="CPU" value="42" unit="%" sparkline={[1, 4, 2, 6]} />,
    )

    const trend = screen.getByRole('img', { name: 'CPU trend' })
    const polyline = container.querySelector('polyline')

    expect(trend).toBeInTheDocument()
    expect(polyline?.getAttribute('points')?.split(' ')).toHaveLength(4)
  })

  it('omits the sparkline when there is only one value', () => {
    render(<MetricCard label="Load" value="2.4" sparkline={[2.4]} />)

    expect(screen.queryByRole('img')).not.toBeInTheDocument()
  })
})
