import {
  formatBytes,
  formatRate,
  formatUptime,
  hottestTemperature,
  isLiveSample,
  minuteSeries,
  rootDisk,
  secondSeries,
  trimToWindow,
  withLivePoint,
  type MetricsPoint,
  type MinuteAggregate,
  type Snapshot,
} from './metrics'

const stat = { avg: 0, max: 0 }

function point(timestamp_ms: number, cpu_percent = 1): MetricsPoint {
  return {
    timestamp_ms,
    cpu_percent,
    memory_used_bytes: 0,
    memory_total_bytes: 0,
    load1: 0,
    rx_bytes_per_sec: 0,
    tx_bytes_per_sec: 0,
    read_bytes_per_sec: 0,
    write_bytes_per_sec: 0,
  }
}

function minute(minute_start_ms: number): MinuteAggregate {
  return {
    minute_start_ms,
    sample_count: 60,
    cpu_percent: stat,
    memory_used_bytes: stat,
    load1: stat,
    rx_bytes_per_sec: stat,
    tx_bytes_per_sec: stat,
    read_bytes_per_sec: stat,
    write_bytes_per_sec: stat,
  }
}

function snapshot(overrides: Partial<Snapshot> = {}): Snapshot {
  return {
    timestamp_ms: 1_000,
    uptime_secs: 10,
    cpu: {
      usage_percent: 25,
      per_core_percent: [20, 30],
      load: { one: 0.5, five: 0.4, fifteen: 0.3 },
    },
    memory: {
      total_bytes: 8 * 1024 ** 3,
      used_bytes: 2 * 1024 ** 3,
      available_bytes: 6 * 1024 ** 3,
      cache_bytes: 0,
      swap_total_bytes: 0,
      swap_used_bytes: 0,
    },
    disks: [],
    network: [],
    temperatures: [],
    top_processes_by_cpu: [],
    top_processes_by_memory: [],
    ...overrides,
  }
}

describe('withLivePoint', () => {
  it('appends a newer point', () => {
    const points = [point(1_000)]

    const result = withLivePoint(points, point(2_000))

    expect(result).toHaveLength(2)
    expect(result[1]?.timestamp_ms).toBe(2_000)
  })

  it('ignores a point at or before the newest timestamp', () => {
    const points = [point(1_000)]

    expect(withLivePoint(points, point(1_000))).toBe(points)
  })
})

describe('trimToWindow', () => {
  it('drops points older than the trailing window', () => {
    const points = [point(1_000), point(2_000), point(3_000)]

    expect(trimToWindow(points, 3_000, 1_000)).toEqual([point(2_000), point(3_000)])
    expect(trimToWindow(points, 3_000, 3_000)).toBe(points)
    expect(trimToWindow([point(1_000)], 10_000, 1_000)).toEqual([])
  })
})

describe('isLiveSample', () => {
  it('accepts a snapshot with its compact point', () => {
    expect(isLiveSample({ snapshot: snapshot(), point: point(1) })).toBe(true)
  })

  it('rejects malformed and legacy payloads', () => {
    expect(isLiveSample(null)).toBe(false)
    expect(isLiveSample({})).toBe(false)
    expect(isLiveSample({ snapshot: {}, point: null })).toBe(false)
    expect(isLiveSample(snapshot())).toBe(false)
  })
})

describe('series helpers', () => {
  it('maps second points and minute averages', () => {
    const seconds = [point(1_000, 10)]
    const minutes = [{ ...minute(60_000), cpu_percent: { avg: 12, max: 20 } }]

    expect(secondSeries(seconds, (entry) => entry.cpu_percent)).toEqual([
      { timestamp: 1_000, value: 10 },
    ])
    expect(minuteSeries(minutes, (entry) => entry.cpu_percent.avg)).toEqual([
      { timestamp: 60_000, value: 12 },
    ])
  })
})

describe('host helpers', () => {
  it('prefers the root mount and falls back to the first', () => {
    const disk = {
      device: 'a',
      device_name: 'a',
      mount_point: '/',
      filesystem: 'ext4',
      total_bytes: 10,
      used_bytes: 5,
      available_bytes: 5,
      read_bytes_per_sec: 0,
      write_bytes_per_sec: 0,
    }
    const other = { ...disk, device: 'b', device_name: 'b', mount_point: '/other' }

    expect(rootDisk(snapshot({ disks: [other, disk] }))?.mount_point).toBe('/')
    expect(rootDisk(snapshot({ disks: [other] }))?.mount_point).toBe('/other')
    expect(rootDisk(snapshot())).toBeUndefined()
  })

  it('finds the warmest sensor', () => {
    expect(
      hottestTemperature(
        snapshot({
          temperatures: [
            { chip: 'a', label: 'x', celsius: 41 },
            { chip: 'a', label: 'y', celsius: 55 },
          ],
        }),
      ),
    ).toBe(55)
    expect(hottestTemperature(snapshot())).toBeUndefined()
  })

  it('formats bytes, rates, and uptime', () => {
    expect(formatBytes(0)).toBe('0 B')
    expect(formatBytes(1536)).toBe('2 KiB')
    expect(formatBytes(2.5 * 1024 ** 3)).toBe('2.5 GiB')
    expect(formatRate(1024)).toBe('1 KiB/s')
    expect(formatUptime(90)).toBe('1m')
    expect(formatUptime(3_600 + 120)).toBe('1h 02m')
    expect(formatUptime(86_400 + 4 * 3_600)).toBe('1d 04h')
  })
})
