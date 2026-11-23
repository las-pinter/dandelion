import type { MetricSeriesPoint } from '@/lib/metrics'

const DEFAULT_MAX_POINTS = 240

/** Averages points into at most `maxPoints` buckets for a stable plot. */
export function downsample(
  points: MetricSeriesPoint[],
  maxPoints = DEFAULT_MAX_POINTS,
): MetricSeriesPoint[] {
  if (maxPoints <= 0 || points.length <= maxPoints) return points

  const bucketSize = points.length / maxPoints
  const result: MetricSeriesPoint[] = []
  for (let bucket = 0; bucket < maxPoints; bucket += 1) {
    const start = Math.floor(bucket * bucketSize)
    const end = Math.min(points.length, Math.max(start + 1, Math.floor((bucket + 1) * bucketSize)))
    const first = points[start]
    if (!first) continue

    let sum = 0
    let count = 0
    for (let index = start; index < end; index += 1) {
      const entry = points[index]
      if (!entry) continue
      sum += entry.value
      count += 1
    }
    result.push({ timestamp: first.timestamp, value: count === 0 ? first.value : sum / count })
  }
  return result
}
