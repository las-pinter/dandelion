import type { ReactNode } from 'react'
import { cn } from '@/lib/utils'
import { StatusPill } from '@/components/StatusPill/StatusPill'
import type { AgentStatus } from '@/components/status'

export interface PanelProps {
  title?: string
  description?: string
  /** When waiting, the panel border takes the waiting tint from the design guide. */
  status?: AgentStatus
  actions?: ReactNode
  className?: string
  children: ReactNode
}

/** A glass panel: the surface that holds metrics, lists, and details. */
export function Panel({ title, description, status, actions, className, children }: PanelProps) {
  const hasHeader = Boolean(title || description || status || actions)
  return (
    <section
      className={cn(
        'rounded-2xl border bg-card p-4 text-card-foreground',
        status === 'waiting' && 'border-status-waiting/40',
        className,
      )}
    >
      {hasHeader && (
        <header className="mb-3 flex flex-wrap items-start justify-between gap-2">
          <div className="min-w-0">
            {title && <h2 className="font-heading text-sm font-semibold">{title}</h2>}
            {description && <p className="mt-0.5 text-xs text-muted-foreground">{description}</p>}
          </div>
          <div className="flex shrink-0 items-center gap-2">
            {status && <StatusPill status={status} />}
            {actions}
          </div>
        </header>
      )}
      {children}
    </section>
  )
}
