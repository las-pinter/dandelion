import { cn } from '@/lib/utils'
import { SPARK_HEIGHT, SPARK_WIDTH, sparklinePoints } from './sparkline'

export interface MetricCardProps {
  label: string
  value: string | number
  unit?: string
  /** Recent values, oldest first. A line appears once there are at least two. */
  sparkline?: number[]
  hint?: string
  className?: string
}

/** One live number with its unit and optional trend line. */
export function MetricCard({ label, value, unit, sparkline, hint, className }: MetricCardProps) {
  const points = sparkline ? sparklinePoints(sparkline) : ''
  return (
    <div className={cn('rounded-xl border bg-card p-4 text-card-foreground', className)}>
      <p className="text-xs font-medium text-muted-foreground">{label}</p>
      <p className="mt-1 flex items-baseline gap-1 font-mono">
        <span className="text-2xl font-semibold tabular-nums">{value}</span>
        {unit && <span className="text-xs text-muted-foreground">{unit}</span>}
      </p>
      {points && (
        <svg
          role="img"
          aria-label={`${label} trend`}
          viewBox={`0 0 ${SPARK_WIDTH} ${SPARK_HEIGHT}`}
          preserveAspectRatio="none"
          className="mt-3 h-9 w-full text-primary"
        >
          <polyline
            points={points}
            fill="none"
            stroke="currentColor"
            strokeWidth={2}
            strokeLinecap="round"
            strokeLinejoin="round"
            vectorEffect="non-scaling-stroke"
          />
        </svg>
      )}
      {hint && <p className="mt-1 text-xs text-muted-foreground">{hint}</p>}
    </div>
  )
}
