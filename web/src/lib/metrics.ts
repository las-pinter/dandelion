import type { components } from '@/api/gen/schema'

export type Snapshot = components['schemas']['Snapshot']
export type MetricsPoint = components['schemas']['MetricsPoint']
export type MinuteAggregate = components['schemas']['MinuteAggregate']
export type DiskMount = components['schemas']['DiskMount']
export type LiveSample = components['schemas']['LiveSample']
export type MetricsRange = '1h' | '7d'

/** Length of the second-resolution window. */
export const HOUR_MS = 60 * 60 * 1000

/** One point on a chart: a timestamp and its value. */
export interface MetricSeriesPoint {
  timestamp: number
  value: number
}

/** Appends a live point unless the stored series already reaches its time. */
export function withLivePoint(points: MetricsPoint[], live: MetricsPoint): MetricsPoint[] {
  const last = points.at(-1)
  if (last && last.timestamp_ms >= live.timestamp_ms) return points
  return [...points, live]
}

/** Keeps only the points inside the trailing window ending at `endMs`. */
export function trimToWindow(
  points: MetricsPoint[],
  endMs: number,
  spanMs: number,
): MetricsPoint[] {
  const cutoff = endMs - spanMs
  const first = points.findIndex((point) => point.timestamp_ms >= cutoff)
  if (first < 0) return []
  if (first === 0) return points
  return points.slice(first)
}

/** Runtime guard for the `system.metrics` payload. */
export function isLiveSample(value: unknown): value is LiveSample {
  if (typeof value !== 'object' || value === null) return false
  const sample = value as Partial<LiveSample>
  return (
    typeof sample.snapshot === 'object' &&
    sample.snapshot !== null &&
    typeof sample.point === 'object' &&
    sample.point !== null &&
    typeof sample.point.timestamp_ms === 'number'
  )
}

/** Maps second-resolution points to a chart series. */
export function secondSeries(
  points: MetricsPoint[],
  pick: (point: MetricsPoint) => number,
): MetricSeriesPoint[] {
  return points.map((point) => ({ timestamp: point.timestamp_ms, value: pick(point) }))
}

/** Maps minute aggregates to a chart series. */
export function minuteSeries(
  minutes: MinuteAggregate[],
  pick: (minute: MinuteAggregate) => number,
): MetricSeriesPoint[] {
  return minutes.map((minute) => ({ timestamp: minute.minute_start_ms, value: pick(minute) }))
}

/** The root filesystem, or the first mount when no root is reported. */
export function rootDisk(snapshot: Snapshot): DiskMount | undefined {
  return snapshot.disks.find((disk) => disk.mount_point === '/') ?? snapshot.disks[0]
}

/** The warmest sensor, if the host exposes any. */
export function hottestTemperature(snapshot: Snapshot): number | undefined {
  if (snapshot.temperatures.length === 0) return undefined
  return snapshot.temperatures.reduce(
    (max, reading) => Math.max(max, reading.celsius),
    Number.NEGATIVE_INFINITY,
  )
}

/** Human-readable byte count. */
export function formatBytes(bytes: number): string {
  const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB']
  let value = bytes
  let unit = 0
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024
    unit += 1
  }
  return `${value.toFixed(unit >= 3 ? 1 : 0)} ${units[unit] ?? 'B'}`
}

/** Human-readable transfer rate. */
export function formatRate(bytesPerSecond: number): string {
  return `${formatBytes(bytesPerSecond)}/s`
}

/** Compact uptime such as `12d 04h` or `3h 07m`. */
export function formatUptime(seconds: number): string {
  const days = Math.floor(seconds / 86_400)
  const hours = Math.floor((seconds % 86_400) / 3_600)
  const minutes = Math.floor((seconds % 3_600) / 60)
  if (days > 0) return `${days}d ${String(hours).padStart(2, '0')}h`
  if (hours > 0) return `${hours}h ${String(minutes).padStart(2, '0')}m`
  return `${minutes}m`
}
