import { Panel } from '@/components/Panel/Panel'
import { Button } from '@/components/ui/button'
import { journalPriority, journalTime, type JournalEntry, type UnitInfo } from '@/lib/services'

export interface JournalPanelProps {
  /** The unit whose journal is shown, or null before a selection. */
  unit: UnitInfo | null
  entries: JournalEntry[]
  loading?: boolean
  error?: string | null
  onRefresh: () => void
}

/** The newest journal entries for the selected unit. */
export function JournalPanel({
  unit,
  entries,
  loading = false,
  error,
  onRefresh,
}: JournalPanelProps) {
  return (
    <Panel
      title="Journal"
      description={unit ? `${unit.name} (${unit.scope})` : 'Select a unit to read its journal'}
      actions={
        unit ? (
          <Button variant="outline" size="sm" onClick={onRefresh}>
            Refresh
          </Button>
        ) : undefined
      }
    >
      {unit && (
        <p className="mb-3 text-xs text-muted-foreground">
          <span className="font-medium text-foreground">{unit.load_state}</span> ·{' '}
          {unit.active_state} ({unit.sub_state})
        </p>
      )}
      {!unit ? (
        <p className="py-6 text-center text-sm text-muted-foreground">
          Select a unit above to read its journal.
        </p>
      ) : loading ? (
        <p className="py-6 text-center text-sm text-muted-foreground">Reading the journal…</p>
      ) : error ? (
        <p role="alert" className="text-sm text-destructive">
          {error}
        </p>
      ) : entries.length === 0 ? (
        <p className="py-6 text-center text-sm text-muted-foreground">No journal entries.</p>
      ) : (
        <ul className="max-h-96 divide-y overflow-y-auto font-mono text-xs">
          {entries.map((entry, index) => (
            <li key={`${entry.timestamp_usec ?? 'none'}-${index}`} className="flex gap-2 py-1.5">
              <span className="shrink-0 text-muted-foreground">
                {journalTime(entry.timestamp_usec ?? null)}
              </span>
              <span className="shrink-0 uppercase text-muted-foreground">
                {journalPriority(entry.priority ?? null)}
              </span>
              <span className="shrink-0 text-muted-foreground">
                {entry.identifier ?? entry.pid ?? '·'}
              </span>
              <span className="break-words whitespace-pre-wrap">{entry.message}</span>
            </li>
          ))}
        </ul>
      )}
    </Panel>
  )
}
