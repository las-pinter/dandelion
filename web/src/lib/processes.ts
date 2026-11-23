import type { components } from '@/api/gen/schema'

export type ProcessInfo = components['schemas']['ProcessInfo']
export type ProcessSignal = components['schemas']['ProcessSignal']
export type ProcessState = components['schemas']['ProcessState']

/** Every scheduling state the API can report, with its display label. */
export const PROCESS_STATE_LABELS: Record<ProcessState, string> = {
  running: 'Running',
  sleeping: 'Sleeping',
  disk_sleep: 'Disk sleep',
  stopped: 'Stopped',
  tracing_stop: 'Tracing',
  zombie: 'Zombie',
  idle: 'Idle',
  dead: 'Dead',
  unknown: 'Unknown',
}

/** True when the process may be signalled or reniced from the UI. */
export function isControllable(process: ProcessInfo): boolean {
  return process.pid > 1
}

/** Total CPU time as a compact duration. */
export function formatCpuTime(seconds: number): string {
  if (seconds < 60) return `${seconds.toFixed(1)}s`
  const minutes = Math.floor(seconds / 60)
  if (minutes < 60) return `${minutes}m ${Math.round(seconds % 60)}s`
  return `${Math.floor(minutes / 60)}h ${minutes % 60}m`
}

/** Case-insensitive match on pid, name, command line, and user. */
export function matchesQuery(process: ProcessInfo, query: string): boolean {
  const needle = query.trim().toLowerCase()
  if (needle === '') return true
  return (
    String(process.pid).includes(needle) ||
    process.name.toLowerCase().includes(needle) ||
    process.command.toLowerCase().includes(needle) ||
    process.user.toLowerCase().includes(needle)
  )
}
