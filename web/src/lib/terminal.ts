import type { components } from '@/api/gen/schema'

export type TerminalInfo = components['schemas']['TerminalInfo']

/** Most shells the server holds open at once; mirrors `MAX_TERMINALS`. */
export const MAX_TERMINALS = 8

/** The socket URL for one terminal on this origin. */
export function terminalSocketUrl(id: string): string {
  const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
  return `${protocol}//${window.location.host}/api/terminals/${id}/ws`
}

/** The control frame that resizes a terminal. */
export function resizeMessage(cols: number, rows: number): string {
  return JSON.stringify({ type: 'resize', cols, rows })
}

/** A short tab label taken from the shell path. */
export function shellLabel(shell: string): string {
  const name = shell.split('/').filter(Boolean).pop()
  return name || shell || 'shell'
}
