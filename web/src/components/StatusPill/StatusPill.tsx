import { cn } from '@/lib/utils'
import { STATUS_META, type AgentStatus } from '@/components/status'

export interface StatusPillProps {
  status: AgentStatus
  className?: string
}

/** Status as a compact pill: colored dot plus the always-visible text label. */
export function StatusPill({ status, className }: StatusPillProps) {
  const meta = STATUS_META[status]
  return (
    <span
      data-status={status}
      className={cn(
        'inline-flex items-center gap-1.5 rounded-full border px-2 py-0.5 text-xs font-medium',
        meta.text,
        meta.border,
        className,
      )}
    >
      <span aria-hidden className={cn('size-1.5 rounded-full', meta.dot, meta.motion)} />
      {meta.label}
    </span>
  )
}
