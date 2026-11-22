export const SPARK_WIDTH = 120
export const SPARK_HEIGHT = 36
const SPARK_PADDING = 2

/** Map values into a viewBox-sized polyline without external chart code. */
export function sparklinePoints(
  values: number[],
  width = SPARK_WIDTH,
  height = SPARK_HEIGHT,
): string {
  if (values.length < 2) return ''
  const min = Math.min(...values)
  const max = Math.max(...values)
  const span = max - min || 1
  const step = width / (values.length - 1)
  return values
    .map((value, index) => {
      const x = index * step
      const y = height - SPARK_PADDING - ((value - min) / span) * (height - SPARK_PADDING * 2)
      return `${x.toFixed(1)},${y.toFixed(1)}`
    })
    .join(' ')
}
