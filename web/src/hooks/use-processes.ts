import { useCallback, useEffect, useRef, useState } from 'react'
import { api } from '@/api/client'
import { messageOf } from '@/lib/api-error'
import type { ProcessInfo, ProcessSignal } from '@/lib/processes'

export interface ProcessesState {
  /** Every process the server can read, sorted by pid. */
  processes: ProcessInfo[]
  /** True while a fetch is running. */
  loading: boolean
  /** The newest fetch or action problem, if any. */
  error: string | null
  /** The pid whose signal or renice is in flight, if any. */
  busyPid: number | null
  /** Fetches the process table again, showing the loading line. */
  refresh: () => void
  /** Sends a signal, then reloads the table. */
  signal: (pid: number, signal: ProcessSignal) => Promise<void>
  /** Changes a nice value, then reloads the table. */
  renice: (pid: number, priority: number) => Promise<void>
}

type FetchResult =
  { processes: ProcessInfo[]; error?: never } | { error: string; processes?: never }

/** Reads the table once, turning every failure into a message. */
async function fetchProcesses(): Promise<FetchResult> {
  try {
    const { data, error } = await api.GET('/api/processes')
    return data ? { processes: data } : { error: messageOf(error, 'the action failed') }
  } catch (cause: unknown) {
    return { error: cause instanceof Error ? cause.message : 'cannot reach the server' }
  }
}

/**
 * Reads the process table and runs the two mutating actions.
 *
 * The list is fetched on mount and after every action; there is no polling,
 * so a page left open keeps the last snapshot until the owner refreshes.
 */
export function useProcesses(): ProcessesState {
  const [processes, setProcesses] = useState<ProcessInfo[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [busyPid, setBusyPid] = useState<number | null>(null)
  const [refreshCount, setRefreshCount] = useState(0)
  // The count is a request token: a response is applied only when its token is
  // still the newest, so an old fetch cannot overwrite a newer one.
  const newestRequest = useRef(0)

  useEffect(() => {
    const token = refreshCount
    newestRequest.current = token
    let ignore = false
    void (async () => {
      const result = await fetchProcesses()
      if (ignore || newestRequest.current !== token) return
      if (result.processes) {
        setProcesses(result.processes)
        setError(null)
      } else {
        setError(result.error)
      }
      setLoading(false)
    })()
    return () => {
      ignore = true
    }
  }, [refreshCount])

  const reload = useCallback(() => setRefreshCount((count) => count + 1), [])

  const refresh = useCallback(() => {
    setLoading(true)
    reload()
  }, [reload])

  const act = useCallback(
    async (pid: number, action: () => Promise<{ error?: unknown }>) => {
      setBusyPid(pid)
      setError(null)
      try {
        const { error: failure } = await action()
        if (failure !== undefined) {
          setError(messageOf(failure, 'the action failed'))
          return
        }
        reload()
      } catch (cause: unknown) {
        setError(cause instanceof Error ? cause.message : 'the action failed')
      } finally {
        setBusyPid(null)
      }
    },
    [reload],
  )

  const signal = useCallback(
    (pid: number, processSignal: ProcessSignal) =>
      act(pid, () => api.POST('/api/processes/signal', { body: { pid, signal: processSignal } })),
    [act],
  )

  const renice = useCallback(
    (pid: number, priority: number) =>
      act(pid, () => api.POST('/api/processes/renice', { body: { pid, priority } })),
    [act],
  )

  return { processes, loading, error, busyPid, refresh, signal, renice }
}
