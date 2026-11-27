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
  UNIT_ACTIONS,
  UNIT_ACTION_LABELS,
  allows,
  unitType,
  type AllowRule,
  type UnitAction,
  type UnitInfo,
} from '@/lib/services'

export interface ServiceTableProps {
  units: UnitInfo[]
  rules: AllowRule[]
  /** The unit whose journal is open, if any. */
  selected: UnitInfo | null
  /** The action in flight, as `scope:unit:action`. */
  busy?: string | null
  onSelect: (unit: UnitInfo) => void
  onAction: (unit: UnitInfo, action: UnitAction) => void
}

const SELECT_CLASS =
  'h-8 rounded-lg border border-input bg-card px-2 text-sm hover:border-muted-foreground focus-visible:border-ring focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring'

/** Unit table with scope, type and state filters and allowlist-gated actions. */
export function ServiceTable({
  units,
  rules,
  selected,
  busy,
  onSelect,
  onAction,
}: ServiceTableProps) {
  const [query, setQuery] = useState('')
  const [scope, setScope] = useState<'all' | 'user' | 'system'>('all')
  const [type, setType] = useState('all')
  const [state, setState] = useState('all')
  const [confirmation, setConfirmation] = useState<{ unit: UnitInfo; action: UnitAction } | null>(
    null,
  )
  const searchId = useId()

  const visible = useMemo(() => {
    const needle = query.trim().toLowerCase()
    return units.filter(
      (unit) =>
        (needle === '' ||
          unit.name.toLowerCase().includes(needle) ||
          unit.description.toLowerCase().includes(needle)) &&
        (scope === 'all' || unit.scope === scope) &&
        (type === 'all' || unitType(unit.name) === type) &&
        (state === 'all' || unit.active_state === state),
    )
  }, [units, query, scope, type, state])

  const request = (unit: UnitInfo, action: UnitAction) => {
    if (CONFIRMED_ACTIONS.has(action)) {
      setConfirmation({ unit, action })
      return
    }
    onAction(unit, action)
  }

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-end gap-2">
        <div className="w-full max-w-xs">
          <label htmlFor={searchId} className="text-xs font-medium text-muted-foreground">
            Search units
          </label>
          <Input
            id={searchId}
            type="search"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="name or description"
            className="mt-1"
          />
        </div>
        <div className="flex flex-wrap items-end gap-2">
          <label className="text-xs font-medium text-muted-foreground">
            Scope
            <select
              aria-label="Scope"
              className={`${SELECT_CLASS} mt-1 block`}
              value={scope}
              onChange={(event) => setScope(event.target.value as typeof scope)}
            >
              <option value="all">All</option>
              <option value="user">User</option>
              <option value="system">System</option>
            </select>
          </label>
          <label className="text-xs font-medium text-muted-foreground">
            Type
            <select
              aria-label="Type"
              className={`${SELECT_CLASS} mt-1 block`}
              value={type}
              onChange={(event) => setType(event.target.value)}
            >
              <option value="all">All</option>
              <option value="service">Service</option>
              <option value="socket">Socket</option>
              <option value="timer">Timer</option>
              <option value="other">Other</option>
            </select>
          </label>
          <label className="text-xs font-medium text-muted-foreground">
            State
            <select
              aria-label="State"
              className={`${SELECT_CLASS} mt-1 block`}
              value={state}
              onChange={(event) => setState(event.target.value)}
            >
              <option value="all">All</option>
              <option value="active">Active</option>
              <option value="inactive">Inactive</option>
              <option value="failed">Failed</option>
            </select>
          </label>
        </div>
        <p className="ml-auto text-xs text-muted-foreground" aria-live="polite">
          {visible.length} of {units.length} units
        </p>
      </div>

      <div className="overflow-x-auto">
        <table className="w-full min-w-[46rem] text-left text-sm">
          <caption className="sr-only">systemd units</caption>
          <thead className="bg-muted/60 text-xs font-medium text-muted-foreground">
            <tr className="border-b">
              <th scope="col" className="py-2 pr-3 font-medium">
                Unit
              </th>
              <th scope="col" className="py-2 pr-3 font-medium">
                Scope
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
            {visible.map((unit) => (
              <tr key={`${unit.scope}:${unit.name}`} className="hover:bg-muted/40">
                <td className="py-2 pr-3">
                  <button
                    type="button"
                    aria-pressed={selected?.scope === unit.scope && selected.name === unit.name}
                    className={SELECT_ROW_CLASS}
                    onClick={() => onSelect(unit)}
                  >
                    {unit.name}
                  </button>
                  <span className="block max-w-[28rem] truncate text-xs text-muted-foreground">
                    {unit.description}
                  </span>
                </td>
                <td className="py-2 pr-3 text-muted-foreground">{unit.scope}</td>
                <td className="py-2 pr-3">
                  {unit.active_state}
                  <span className="block text-xs text-muted-foreground">{unit.sub_state}</span>
                </td>
                <td className="py-2">
                  <div className="flex flex-wrap gap-1">
                    {UNIT_ACTIONS.map((action) => {
                      const permitted = allows(rules, unit.scope, unit.name, action)
                      const inFlight = busy === `${unit.scope}:${unit.name}:${action}`
                      return (
                        <Button
                          key={action}
                          variant={
                            action === 'stop' || action === 'disable' ? 'destructive' : 'outline'
                          }
                          size="xs"
                          disabled={!permitted || inFlight}
                          title={permitted ? undefined : 'not in the sudo allowlist'}
                          aria-label={`${UNIT_ACTION_LABELS[action]} ${unit.name}`}
                          onClick={() => request(unit, action)}
                        >
                          {UNIT_ACTION_LABELS[action]}
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
                  {units.length === 0 ? 'No units are readable.' : 'No units match the filters.'}
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
                ? `${UNIT_ACTION_LABELS[confirmation.action]} ${confirmation.unit.name}?`
                : ''}
            </DialogTitle>
            <DialogDescription>
              {confirmation === null
                ? ''
                : confirmation.unit.scope === 'system'
                  ? `This changes the system unit ${confirmation.unit.name}. Unlisted system units are refused.`
                  : `This changes the user unit ${confirmation.unit.name}.`}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter showCloseButton>
            <Button
              variant={confirmation?.action === 'stop' ? 'destructive' : 'default'}
              onClick={() => {
                if (!confirmation) return
                onAction(confirmation.unit, confirmation.action)
                setConfirmation(null)
              }}
            >
              {confirmation ? UNIT_ACTION_LABELS[confirmation.action] : 'Run'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}
