import { cn } from '@/lib/utils'
import { STATUS_META, type AgentStatus } from '@/components/status'

export interface StatusDotProps {
  status: AgentStatus
  /** Show the text label beside the dot instead of only to screen readers. */
  withLabel?: boolean
  className?: string
}

/** A status dot; the label is always available as text, visible or sr-only. */
export function StatusDot({ status, withLabel = false, className }: StatusDotProps) {
  const meta = STATUS_META[status]
  return (
    <span className={cn('inline-flex items-center gap-1.5', className)}>
      <span
        aria-hidden
        data-status={status}
        className={cn('size-2.5 shrink-0 rounded-full', meta.dot, meta.motion)}
      />
      {withLabel ? (
        <span className={cn('text-xs font-medium', meta.text)}>{meta.label}</span>
      ) : (
        <span className="sr-only">{meta.label}</span>
      )}
    </span>
  )
}
