import type { components } from '@/api/gen/schema'

export type ContainersSnapshot = components['schemas']['ContainersSnapshot']
export type ContainerInfo = components['schemas']['ContainerInfo']
export type ContainerState = components['schemas']['ContainerState']
export type ContainerAction = components['schemas']['ContainerAction']
export type ContainerLogLine = components['schemas']['ContainerLogLine']
export type DockerAvailability = components['schemas']['DockerAvailability']

/** Every action the API accepts, in menu order. */
export const CONTAINER_ACTIONS: ContainerAction[] = ['start', 'stop', 'restart']

/** Display label for one action. */
export const CONTAINER_ACTION_LABELS: Record<ContainerAction, string> = {
  start: 'Start',
  stop: 'Stop',
  restart: 'Restart',
}

/** Actions that interrupt a running container and therefore ask first. */
export const CONFIRMED_ACTIONS: ReadonlySet<ContainerAction> = new Set(['stop', 'restart'])

/** True when the container is up, or on its way up or down. */
export function isRunning(state: ContainerState): boolean {
  return state === 'running' || state === 'restarting' || state === 'paused'
}

/** True when the container's state accepts the action. */
export function canAct(state: ContainerState, action: ContainerAction): boolean {
  switch (action) {
    case 'start':
      return state === 'created' || state === 'exited'
    case 'stop':
      return isRunning(state)
    case 'restart':
      return isRunning(state) || state === 'exited'
  }
}

/** The container name, or a short id when Docker reported none. */
export function containerLabel(container: ContainerInfo): string {
  return container.name || container.id.slice(0, 12) || 'unknown'
}

/**
 * Splits Docker's timestamp prefix from a log line when timestamps are asked
 * for. A line without one keeps its text and reports no timestamp.
 */
export function splitLogTimestamp(message: string): { timestamp: string | null; text: string } {
  const match = /^(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?Z)\s([\s\S]*)$/.exec(message)
  const timestamp = match?.[1]
  if (timestamp === undefined) return { timestamp: null, text: message }
  return { timestamp, text: match?.[2] ?? '' }
}

/** A Docker timestamp as local time, or an em dash when absent. */
export function logTime(timestamp: string | null): string {
  if (timestamp === null) return '—'
  const date = new Date(timestamp)
  return Number.isNaN(date.getTime()) ? timestamp : date.toLocaleTimeString()
}
