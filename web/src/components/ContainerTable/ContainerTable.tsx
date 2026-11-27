import { useId, useMemo, useState } from 'react'
import { Button } from '@/components/ui/button'
import { SELECT_ROW_CLASS } from '@/lib/row-control'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import {
  CONFIRMED_ACTIONS,
  CONTAINER_ACTIONS,
  CONTAINER_ACTION_LABELS,
  canAct,
  containerLabel,
  isRunning,
  type ContainerAction,
  type ContainerInfo,
} from '@/lib/containers'

export interface ContainerTableProps {
  containers: ContainerInfo[]
  /** The container whose logs are open, if any. */
  selected: ContainerInfo | null
  /** The action in flight, as `container:action`. */
  busy?: string | null
  onSelect: (container: ContainerInfo) => void
  onAction: (container: ContainerInfo, action: ContainerAction) => void
}

const SELECT_CLASS =
  'h-8 rounded-lg border border-input bg-card px-2 text-sm hover:border-muted-foreground focus-visible:border-ring focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring'

/** Container table with a text and state filter and state-gated actions. */
export function ContainerTable({
  containers,
  selected,
  busy,
  onSelect,
  onAction,
}: ContainerTableProps) {
  const [query, setQuery] = useState('')
  const [show, setShow] = useState<'all' | 'running' | 'stopped'>('all')
  const [confirmation, setConfirmation] = useState<{
    container: ContainerInfo
    action: ContainerAction
  } | null>(null)
  const searchId = useId()

  const visible = useMemo(() => {
    const needle = query.trim().toLowerCase()
    return containers.filter((container) => {
      const name = containerLabel(container).toLowerCase()
      return (
        (needle === '' ||
          name.includes(needle) ||
          container.image.toLowerCase().includes(needle) ||
          container.id.startsWith(needle)) &&
        (show === 'all' || (show === 'running') === isRunning(container.state))
      )
    })
  }, [containers, query, show])

  const request = (container: ContainerInfo, action: ContainerAction) => {
    if (CONFIRMED_ACTIONS.has(action)) {
      setConfirmation({ container, action })
      return
    }
    onAction(container, action)
  }

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-end gap-2">
        <div className="w-full max-w-xs">
          <label htmlFor={searchId} className="text-xs font-medium text-muted-foreground">
            Search containers
          </label>
          <Input
            id={searchId}
            type="search"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="name, image, or id"
            className="mt-1"
          />
        </div>
        <label className="text-xs font-medium text-muted-foreground">
          Show
          <select
            aria-label="Show"
            className={`${SELECT_CLASS} mt-1 block`}
            value={show}
            onChange={(event) => setShow(event.target.value as typeof show)}
          >
            <option value="all">All</option>
            <option value="running">Running</option>
            <option value="stopped">Stopped</option>
          </select>
        </label>
        <p className="ml-auto text-xs text-muted-foreground" aria-live="polite">
          {visible.length} of {containers.length} containers
        </p>
      </div>

      <div className="overflow-x-auto">
        <table className="w-full min-w-[46rem] text-left text-sm">
          <caption className="sr-only">Docker containers</caption>
          <thead className="bg-muted/60 text-xs font-medium text-muted-foreground">
            <tr className="border-b">
              <th scope="col" className="py-2 pr-3 font-medium">
                Container
              </th>
              <th scope="col" className="py-2 pr-3 font-medium">
                Image
              </th>
              <th scope="col" className="py-2 pr-3 font-medium">
                State
              </th>
              <th scope="col" className="py-2 font-medium">
                Actions
              </th>
            </tr>
          </thead>
          <tbody className="divide-y">
            {visible.map((container) => (
              <tr key={container.id || containerLabel(container)} className="hover:bg-muted/40">
                <td className="py-2 pr-3">
                  <button
                    type="button"
                    aria-pressed={selected?.id === container.id}
                    className={SELECT_ROW_CLASS}
                    onClick={() => onSelect(container)}
                  >
                    {containerLabel(container)}
                  </button>
                  <span className="block font-mono text-xs text-muted-foreground">
                    {container.id.slice(0, 12)}
                  </span>
                </td>
                <td className="py-2 pr-3">
                  <span className="block max-w-[20rem] truncate">{container.image}</span>
                  {container.command && (
                    <span className="block max-w-[20rem] truncate font-mono text-xs text-muted-foreground">
                      {container.command}
                    </span>
                  )}
                </td>
                <td className="py-2 pr-3">
                  {container.state}
                  <span className="block text-xs text-muted-foreground">{container.status}</span>
                </td>
                <td className="py-2">
                  <div className="flex flex-wrap gap-1">
                    {CONTAINER_ACTIONS.map((action) => {
                      const permitted = canAct(container.state, action)
                      const label = containerLabel(container)
                      const inFlight = busy === `${label}:${action}`
                      return (
                        <Button
                          key={action}
                          variant={
                            action === 'stop' || action === 'restart' ? 'destructive' : 'outline'
                          }
                          size="xs"
                          disabled={!permitted || inFlight}
                          title={permitted ? undefined : `not allowed while ${container.state}`}
                          aria-label={`${CONTAINER_ACTION_LABELS[action]} ${label}`}
                          onClick={() => request(container, action)}
                        >
                          {CONTAINER_ACTION_LABELS[action]}
                        </Button>
                      )
                    })}
                  </div>
                </td>
              </tr>
            ))}
            {visible.length === 0 && (
              <tr>
                <td colSpan={4} className="py-8 text-center text-sm text-muted-foreground">
                  {containers.length === 0
                    ? 'No containers are readable.'
                    : 'No containers match the filters.'}
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>

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
                ? `${CONTAINER_ACTION_LABELS[confirmation.action]} ${containerLabel(confirmation.container)}?`
                : ''}
            </DialogTitle>
            <DialogDescription>
              {confirmation === null
                ? ''
                : `${CONTAINER_ACTION_LABELS[confirmation.action]} the container ${containerLabel(confirmation.container)}.`}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter showCloseButton>
            <Button
              variant={confirmation?.action === 'stop' ? 'destructive' : 'default'}
              onClick={() => {
                if (!confirmation) return
                onAction(confirmation.container, confirmation.action)
                setConfirmation(null)
              }}
            >
              {confirmation ? CONTAINER_ACTION_LABELS[confirmation.action] : 'Run'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}
