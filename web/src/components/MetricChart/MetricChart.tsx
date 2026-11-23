import { cn } from '@/lib/utils'
import type { MetricSeriesPoint } from '@/lib/metrics'
import { downsample } from './downsample'

const WIDTH = 640
const HEIGHT = 180
const PADDING = { top: 12, right: 12, bottom: 24, left: 44 }

export interface MetricChartProps {
  /** Series name shown above the plot. */
  label: string
  /** Points, oldest first. */
  points: MetricSeriesPoint[]
  /** Unit appended to the current value and the axis labels. */
  unit?: string
  /** Fixed y-axis bounds; otherwise the data range is used. */
  domain?: [number, number]
  /** Most points drawn; longer series are averaged into buckets. */
  maxPoints?: number
  /** CSS color for the line and area. */
  color?: string
  className?: string
}

/** One live series with its current value and a trend line. */
export function MetricChart({
  label,
  points,
  unit,
  domain,
  maxPoints,
  color = 'var(--chart-1)',
  className,
}: MetricChartProps) {
  const series = downsample(points, maxPoints)
  const last = series.at(-1)

  return (
    <figure className={cn('min-w-0', className)}>
      <figcaption className="flex flex-wrap items-baseline justify-between gap-x-2">
        <span className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
          {label}
        </span>
        <span className="font-mono text-sm tabular-nums">
          {last ? formatValue(last.value, unit) : '—'}
          {unit && last && <span className="ml-1 text-xs text-muted-foreground">{unit}</span>}
        </span>
      </figcaption>
      {series.length < 2 ? (
        <p className="mt-2 flex h-40 items-center justify-center text-xs text-muted-foreground">
          Waiting for samples…
        </p>
      ) : (
        <Plot points={series} label={label} unit={unit} domain={domain} color={color} />
      )}
    </figure>
  )
}

interface PlotProps {
  points: MetricSeriesPoint[]
  label: string
  unit?: string
  domain?: [number, number]
  color: string
}

function Plot({ points, label, unit, domain, color }: PlotProps) {
  const first = points[0]
  const last = points.at(-1)
  if (!first || !last) return null

  const values = points.map((point) => point.value)
  const dataMin = Math.min(...values)
  const dataMax = Math.max(...values)
  const [min, max] = domain ?? [dataMin, dataMax]
  const span = max - min || 1

  const chartWidth = WIDTH - PADDING.left - PADDING.right
  const chartHeight = HEIGHT - PADDING.top - PADDING.bottom
  const timeSpan = last.timestamp - first.timestamp || 1

  const x = (timestamp: number) =>
    PADDING.left + ((timestamp - first.timestamp) / timeSpan) * chartWidth
  const y = (value: number) => {
    const ratio = Math.min(1, Math.max(0, (value - min) / span))
    return PADDING.top + (1 - ratio) * chartHeight
  }

  const coords = points.map((point) => [x(point.timestamp), y(point.value)] as const)
  const line = coords.map(([cx, cy]) => `${cx.toFixed(1)},${cy.toFixed(1)}`).join(' ')
  const firstCoord = coords[0]
  const lastCoord = coords.at(-1)
  if (!firstCoord || !lastCoord) return null
  const baseline = PADDING.top + chartHeight
  const area = `M${firstCoord[0].toFixed(1)},${baseline} L${coords
    .map(([cx, cy]) => `${cx.toFixed(1)},${cy.toFixed(1)}`)
    .join(' L')} L${lastCoord[0].toFixed(1)},${baseline} Z`

  const ticks = [max, min + span / 2, min]
  const longSpan = timeSpan > 24 * 60 * 60 * 1000

  return (
    <svg
      viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
      role="img"
      aria-label={`${label} history, ${points.length} points`}
      className="mt-2 h-40 w-full"
      style={{ color }}
    >
      {ticks.map((tick) => (
        <line
          key={tick}
          x1={PADDING.left}
          x2={WIDTH - PADDING.right}
          y1={y(tick)}
          y2={y(tick)}
          stroke="currentColor"
          strokeOpacity={0.12}
        />
      ))}
      <path d={area} fill="currentColor" fillOpacity={0.1} />
      <polyline
        points={line}
        fill="none"
        stroke="currentColor"
        strokeWidth={2}
        strokeLinecap="round"
        strokeLinejoin="round"
        vectorEffect="non-scaling-stroke"
      />
      {ticks.map((tick) => (
        <text
          key={`label-${tick}`}
          x={PADDING.left - 6}
          y={y(tick) + 3}
          textAnchor="end"
          fontSize={10}
          fill="currentColor"
          fillOpacity={0.55}
          className="font-mono"
        >
          {formatValue(tick, unit)}
        </text>
      ))}
      <text
        x={PADDING.left}
        y={HEIGHT - 6}
        fontSize={10}
        fill="currentColor"
        fillOpacity={0.55}
        className="font-mono"
      >
        {formatTime(first.timestamp, longSpan)}
      </text>
      <text
        x={WIDTH - PADDING.right}
        y={HEIGHT - 6}
        textAnchor="end"
        fontSize={10}
        fill="currentColor"
        fillOpacity={0.55}
        className="font-mono"
      >
        {formatTime(last.timestamp, longSpan)}
      </text>
    </svg>
  )
}

function formatValue(value: number, unit?: string): string {
  if (unit === '%') return String(Math.round(value))
  if (Math.abs(value) >= 100) return value.toFixed(0)
  if (Math.abs(value) >= 10) return value.toFixed(1)
  return value.toFixed(2)
}

function formatTime(timestamp: number, longSpan: boolean): string {
  const date = new Date(timestamp)
  return longSpan
    ? date.toLocaleDateString(undefined, { month: 'short', day: 'numeric' })
    : date.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' })
}
