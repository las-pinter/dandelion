import { useCallback, useState } from 'react'
import { api } from '@/api/client'
import { messageOf } from '@/lib/api-error'
import type { TerminalInfo } from '@/lib/terminal'

export interface TerminalsState {
  /** The newest open or close problem. */
  error: string | null
  /** True while a shell is opening. */
  busy: boolean
  /** Opens a shell, returning it on success. */
  open: (cols?: number, rows?: number) => Promise<TerminalInfo | null>
  /** Closes one shell. */
  close: (id: string) => Promise<void>
}

/**
 * Opens and closes the server's shells.
 *
 * Each shell belongs to the socket its view opens, so there is no listing and
 * no polling: a shell left by another page ends when that page's socket does.
 */
export function useTerminals(): TerminalsState {
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const open = useCallback(async (cols?: number, rows?: number) => {
    setBusy(true)
    setError(null)
    try {
      const { data, error: failure } = await api.POST('/api/terminals', {
        body: { cols, rows },
      })
      if (data) return data
      setError(messageOf(failure))
      return null
    } catch (cause: unknown) {
      setError(cause instanceof Error ? cause.message : 'the shell could not be opened')
      return null
    } finally {
      setBusy(false)
    }
  }, [])

  const close = useCallback(async (id: string) => {
    try {
      const { error: failure } = await api.DELETE('/api/terminals/{id}', {
        params: { path: { id } },
      })
      // A shell that already ended is as closed as one this call ended.
      if (failure !== undefined && !isNotFound(failure)) setError(messageOf(failure))
    } catch (cause: unknown) {
      setError(cause instanceof Error ? cause.message : 'the shell could not be closed')
    }
  }, [])

  return { error, busy, open, close }
}

/** True when the API refused because the terminal is already gone. */
function isNotFound(failure: unknown): boolean {
  return (
    typeof failure === 'object' &&
    failure !== null &&
    'error' in failure &&
    failure.error === 'not_found'
  )
}
