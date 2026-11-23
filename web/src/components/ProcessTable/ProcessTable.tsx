import { useId, useMemo, useState } from 'react'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { formatBytes } from '@/lib/metrics'
import {
  PROCESS_STATE_LABELS,
  formatCpuTime,
  isControllable,
  matchesQuery,
  type ProcessInfo,
  type ProcessSignal,
} from '@/lib/processes'

export interface ProcessTableProps {
  processes: ProcessInfo[]
  /** True while the table is loading. */
  loading?: boolean
  /** The newest fetch or action problem. */
  error?: string | null
  /** The pid whose action is in flight, if any. */
  busyPid?: number | null
  onSignal: (pid: number, signal: ProcessSignal) => void
  onRenice: (pid: number, priority: number) => void
}

interface Confirmation {
  process: ProcessInfo
  signal: ProcessSignal
}

const SIGNAL_LABELS: Record<ProcessSignal, { verb: string; description: string }> = {
  term: { verb: 'Terminate', description: 'Send SIGTERM so the process can shut down cleanly.' },
  kill: {
    verb: 'Force kill',
    description: 'Send SIGKILL. The process cannot clean up after itself.',
  },
  int: { verb: 'Interrupt', description: 'Send SIGINT, as Ctrl+C does.' },
  stop: { verb: 'Stop', description: 'Send SIGSTOP. The process freezes until continued.' },
  cont: { verb: 'Continue', description: 'Send SIGCONT to resume a stopped process.' },
}

/** Searchable process table with signal and renice actions. */
export function ProcessTable({
  processes,
  loading = false,
  error,
  busyPid,
  onSignal,
  onRenice,
}: ProcessTableProps) {
  const [query, setQuery] = useState('')
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null)
  const [renicing, setRenicing] = useState<ProcessInfo | null>(null)
  const [priority, setPriority] = useState('0')
  const searchId = useId()

  const visible = useMemo(
    () => processes.filter((process) => matchesQuery(process, query)),
    [processes, query],
  )

  const openRenice = (process: ProcessInfo) => {
    setPriority(String(process.niceness))
    setRenicing(process)
  }

  const parsedPriority = Number(priority)
  const priorityValid =
    priority.trim() !== '' &&
    Number.isInteger(parsedPriority) &&
    parsedPriority >= -20 &&
    parsedPriority <= 19

  const loadingFirst = loading && processes.length === 0

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-end justify-between gap-2">
        <div className="w-full max-w-sm">
          <label htmlFor={searchId} className="text-xs font-medium text-muted-foreground">
            Search processes
          </label>
          <Input
            id={searchId}
            type="search"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="pid, name, command, user"
            className="mt-1"
          />
        </div>
        <p className="text-xs text-muted-foreground" aria-live="polite">
          {visible.length} of {processes.length} processes
        </p>
      </div>

      {error && (
        <p role="alert" className="text-sm text-destructive">
          {error}
        </p>
      )}

      {loadingFirst ? (
        <p className="py-8 text-center text-sm text-muted-foreground">Reading processes…</p>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full min-w-[44rem] text-left text-sm">
            <caption className="sr-only">Processes on this machine</caption>
            <thead className="text-xs uppercase tracking-wide text-muted-foreground">
              <tr className="border-b">
                <th scope="col" className="py-2 pr-3 font-medium">
                  PID
                </th>
                <th scope="col" className="py-2 pr-3 font-medium">
                  Process
                </th>
                <th scope="col" className="py-2 pr-3 font-medium">
                  User
                </th>
                <th scope="col" className="py-2 pr-3 font-medium">
                  State
                </th>
                <th scope="col" className="py-2 pr-3 text-right font-medium">
                  CPU time
                </th>
                <th scope="col" className="py-2 pr-3 text-right font-medium">
                  Memory
                </th>
                <th scope="col" className="py-2 pr-3 text-right font-medium">
                  Nice
                </th>
                <th scope="col" className="py-2 font-medium">
                  Actions
                </th>
              </tr>
            </thead>
            <tbody className="divide-y">
              {visible.map((process) => {
                const controllable = isControllable(process)
                const busy = busyPid === process.pid
                return (
                  <tr key={process.pid} className={busy ? 'opacity-60' : undefined}>
                    <td className="py-2 pr-3 font-mono text-xs tabular-nums">{process.pid}</td>
                    <td className="py-2 pr-3">
                      <span className="block font-medium">{process.name}</span>
                      <span
                        className="block max-w-[28rem] truncate font-mono text-xs text-muted-foreground"
                        title={process.command}
                      >
                        {process.command}
                      </span>
                    </td>
                    <td className="py-2 pr-3 text-muted-foreground">{process.user}</td>
                    <td className="py-2 pr-3">{PROCESS_STATE_LABELS[process.state]}</td>
                    <td className="py-2 pr-3 text-right font-mono text-xs tabular-nums">
                      {formatCpuTime(process.cpu_seconds)}
                    </td>
                    <td className="py-2 pr-3 text-right font-mono text-xs tabular-nums">
                      {formatBytes(process.memory_bytes)}
                    </td>
                    <td className="py-2 pr-3 text-right font-mono text-xs tabular-nums">
                      {process.niceness}
                    </td>
                    <td className="py-2">
                      <div className="flex flex-wrap gap-1">
                        <Button
                          variant="outline"
                          size="xs"
                          disabled={!controllable || busy}
                          aria-label={`Terminate ${process.name} (${process.pid})`}
                          onClick={() => setConfirmation({ process, signal: 'term' })}
                        >
                          Term
                        </Button>
                        <Button
                          variant="destructive"
                          size="xs"
                          disabled={!controllable || busy}
                          aria-label={`Force kill ${process.name} (${process.pid})`}
                          onClick={() => setConfirmation({ process, signal: 'kill' })}
                        >
                          Kill
                        </Button>
                        <Button
                          variant="ghost"
                          size="xs"
                          disabled={!controllable || busy}
                          aria-label={`Renice ${process.name} (${process.pid})`}
                          onClick={() => openRenice(process)}
                        >
                          Renice
                        </Button>
                      </div>
                    </td>
                  </tr>
                )
              })}
              {visible.length === 0 && (
                <tr>
                  <td colSpan={8} className="py-8 text-center text-sm text-muted-foreground">
                    {query.trim() === ''
                      ? 'No processes are readable.'
                      : `No processes match “${query.trim()}”.`}
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      )}

      <Dialog
        open={confirmation !== null}
        onOpenChange={(open) => {
          if (!open) setConfirmation(null)
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>
              {confirmation
                ? `${SIGNAL_LABELS[confirmation.signal].verb} ${confirmation.process.name}?`
                : ''}
            </DialogTitle>
            <DialogDescription>
              {confirmation
                ? `PID ${confirmation.process.pid}. ${SIGNAL_LABELS[confirmation.signal].description}`
                : ''}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter showCloseButton>
            <Button
              variant={confirmation?.signal === 'kill' ? 'destructive' : 'default'}
              onClick={() => {
                if (!confirmation) return
                onSignal(confirmation.process.pid, confirmation.signal)
                setConfirmation(null)
              }}
            >
              {confirmation ? SIGNAL_LABELS[confirmation.signal].verb : 'Send'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={renicing !== null}
        onOpenChange={(open) => {
          if (!open) setRenicing(null)
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Renice {renicing?.name}</DialogTitle>
            <DialogDescription>
              A higher nice value lowers the process's priority. Negative values need CAP_SYS_NICE.
            </DialogDescription>
          </DialogHeader>
          <div className="flex flex-col gap-1">
            <label htmlFor={`${searchId}-priority`} className="text-xs font-medium">
              Nice value (-20 to 19)
            </label>
            <Input
              id={`${searchId}-priority`}
              type="number"
              min={-20}
              max={19}
              value={priority}
              onChange={(event) => setPriority(event.target.value)}
              aria-invalid={priorityValid ? undefined : true}
            />
            {!priorityValid && (
              <p className="text-xs text-destructive">Enter a whole number from -20 to 19.</p>
            )}
          </div>
          <DialogFooter showCloseButton>
            <Button
              disabled={!priorityValid}
              onClick={() => {
                if (!renicing || !priorityValid) return
                onRenice(renicing.pid, parsedPriority)
                setRenicing(null)
              }}
            >
              Save
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}
