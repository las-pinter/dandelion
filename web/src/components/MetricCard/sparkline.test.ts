import { SPARK_HEIGHT, SPARK_WIDTH, sparklinePoints } from './sparkline'

describe('sparklinePoints', () => {
  it('returns nothing for fewer than two values', () => {
    expect(sparklinePoints([])).toBe('')
    expect(sparklinePoints([4])).toBe('')
  })

  it('spans the full width and keeps values inside the height', () => {
    const points = sparklinePoints([0, 10, 5]).split(' ')

    expect(points[0]).toBe('0.0,34.0')
    expect(points[1]).toBe('60.0,2.0')
    expect(points[2]).toBe('120.0,18.0')
  })

  it('flattens a series with no range without dividing by zero', () => {
    const points = sparklinePoints([3, 3, 3])

    expect(points).toBe(
      `0.0,${SPARK_HEIGHT - 2}.0 60.0,${SPARK_HEIGHT - 2}.0 ${SPARK_WIDTH}.0,${SPARK_HEIGHT - 2}.0`,
    )
  })
})
