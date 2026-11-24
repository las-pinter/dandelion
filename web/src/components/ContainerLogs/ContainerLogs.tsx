import { Panel } from '@/components/Panel/Panel'
import { Button } from '@/components/ui/button'
import {
  containerLabel,
  logTime,
  splitLogTimestamp,
  type ContainerInfo,
  type ContainerLogLine,
} from '@/lib/containers'

export interface ContainerLogsProps {
  /** The container whose logs are shown, or null before a selection. */
  container: ContainerInfo | null
  lines: ContainerLogLine[]
  loading?: boolean
  error?: string | null
  onRefresh: () => void
}

/** The newest log lines for the selected container. */
export function ContainerLogs({
  container,
  lines,
  loading = false,
  error,
  onRefresh,
}: ContainerLogsProps) {
  return (
    <Panel
      title="Logs"
      description={
        container
          ? `${containerLabel(container)} · ${container.image}`
          : 'Select a container to read its logs'
      }
      actions={
        container ? (
          <Button variant="outline" size="sm" onClick={onRefresh}>
            Refresh
          </Button>
        ) : undefined
      }
    >
      {container && (
        <p className="mb-3 text-xs text-muted-foreground">
          <span className="font-medium text-foreground">{container.state}</span> ·{' '}
          {container.status}
        </p>
      )}
      {!container ? (
        <p className="py-6 text-center text-sm text-muted-foreground">
          Select a container above to read its logs.
        </p>
      ) : loading ? (
        <p className="py-6 text-center text-sm text-muted-foreground">Reading the logs…</p>
      ) : error ? (
        <p role="alert" className="text-sm text-destructive">
          {error}
        </p>
      ) : lines.length === 0 ? (
        <p className="py-6 text-center text-sm text-muted-foreground">No log lines.</p>
      ) : (
        <ul className="max-h-96 divide-y overflow-y-auto font-mono text-xs">
          {lines.map((line, index) => {
            const { timestamp, text } = splitLogTimestamp(line.message)
            return (
              <li key={`${timestamp ?? 'none'}-${index}`} className="flex gap-2 py-1.5">
                <span className="shrink-0 text-muted-foreground">{logTime(timestamp)}</span>
                <span className="shrink-0 uppercase text-muted-foreground">{line.stream}</span>
                <span className="break-words whitespace-pre-wrap">{text}</span>
              </li>
            )
          })}
        </ul>
      )}
    </Panel>
  )
}
