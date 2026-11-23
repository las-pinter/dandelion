import { render, screen, within } from '@testing-library/react'
import { MetricChart } from './MetricChart'
import { downsample } from './downsample'

function series(count: number): { timestamp: number; value: number }[] {
  return Array.from({ length: count }, (_, index) => ({
    timestamp: 1_700_000_000_000 + index * 1_000,
    value: index,
  }))
}

describe('downsample', () => {
  it('keeps a short series unchanged', () => {
    const points = series(10)

    expect(downsample(points, 100)).toBe(points)
  })

  it('averages a long series into the requested buckets', () => {
    const points = series(1_000)

    const result = downsample(points, 100)

    expect(result).toHaveLength(100)
    expect(result[0]?.value).toBeCloseTo(4.5)
  })
})

describe('MetricChart', () => {
  it('draws a line, its value, and an accessible label', () => {
    const { container } = render(
      <MetricChart label="CPU" unit="%" points={series(3)} domain={[0, 2]} />,
    )

    expect(screen.getByRole('img', { name: 'CPU history, 3 points' })).toBeInTheDocument()
    expect(container.querySelector('polyline')?.getAttribute('points')?.split(' ')).toHaveLength(3)
    expect(
      within(container.querySelector('figcaption') as HTMLElement).getByText('2'),
    ).toBeInTheDocument()
    expect(screen.getByText('%')).toBeInTheDocument()
  })

  it('waits until there are two points', () => {
    render(<MetricChart label="Memory" unit="GiB" points={series(1)} />)

    expect(screen.getByText('Waiting for samples…')).toBeInTheDocument()
    expect(screen.queryByRole('img')).not.toBeInTheDocument()
  })

  it('shows an em dash before the first sample', () => {
    render(<MetricChart label="Load" points={[]} />)

    expect(screen.getByText('—')).toBeInTheDocument()
  })
})
