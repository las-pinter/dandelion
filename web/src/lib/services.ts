import type { components } from '@/api/gen/schema'

export type ServicesSnapshot = components['schemas']['ServicesSnapshot']
export type UnitInfo = components['schemas']['UnitInfo']
export type UnitScope = components['schemas']['UnitScope']
export type UnitAction = components['schemas']['UnitAction']
export type AllowRule = components['schemas']['AllowRule']
export type JournalEntry = components['schemas']['JournalEntry']

/** Every action the API accepts, in menu order. */
export const UNIT_ACTIONS: UnitAction[] = ['start', 'stop', 'restart', 'enable', 'disable']

/** Display label for one action. */
export const UNIT_ACTION_LABELS: Record<UnitAction, string> = {
  start: 'Start',
  stop: 'Stop',
  restart: 'Restart',
  enable: 'Enable',
  disable: 'Disable',
}

/** Actions that interrupt a running unit and therefore ask first. */
export const CONFIRMED_ACTIONS: ReadonlySet<UnitAction> = new Set(['stop', 'restart', 'disable'])

/** The unit suffix without the leading dot, such as `service`. */
export function unitType(name: string): string {
  const dot = name.lastIndexOf('.')
  return dot < 0 ? 'other' : name.slice(dot + 1)
}

/** True when a unit's scope and allowlist permit the action. */
export function allows(
  rules: AllowRule[],
  scope: UnitScope,
  unit: string,
  action: UnitAction,
): boolean {
  if (scope === 'user') return true
  return rules.some((rule) => rule.unit === unit && rule.verbs.includes(action))
}

/** A journal timestamp as local time, or an em dash when absent. */
export function journalTime(timestamp_usec: number | null): string {
  if (timestamp_usec === null) return '—'
  return new Date(timestamp_usec / 1000).toLocaleTimeString()
}

/** Syslog severity label for a priority value, if it is known. */
export function journalPriority(priority: number | null): string {
  const labels = ['emerg', 'alert', 'crit', 'err', 'warning', 'notice', 'info', 'debug']
  return priority === null ? '—' : (labels[priority] ?? '—')
}
