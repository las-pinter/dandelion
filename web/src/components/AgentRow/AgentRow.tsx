import { cn } from '@/lib/utils'
import { StatusPill } from '@/components/StatusPill/StatusPill'
import type { AgentStatus } from '@/components/status'

export interface AgentRowProps {
  name: string
  status: AgentStatus
  /** The command the agent is running, shown in mono. */
  command?: string
  /** Short right-aligned detail, such as an elapsed time. */
  detail?: string
  className?: string
}

/** One agent in a list: name, command, state, and an optional detail. */
export function AgentRow({ name, status, command, detail, className }: AgentRowProps) {
  return (
    <div className={cn('flex min-h-11 items-center justify-between gap-3 py-2', className)}>
      <div className="min-w-0">
        <p className="truncate text-sm font-medium">{name}</p>
        {command && <p className="truncate font-mono text-xs text-muted-foreground">{command}</p>}
      </div>
      <div className="flex shrink-0 items-center gap-2">
        {detail && (
          <span className="hidden font-mono text-xs text-muted-foreground sm:inline">{detail}</span>
        )}
        <StatusPill status={status} />
      </div>
    </div>
  )
}
