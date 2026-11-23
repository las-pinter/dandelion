import { useCallback, useEffect, useRef, useState } from 'react'
import { api } from '@/api/client'
import { messageOf } from '@/lib/api-error'
import type { ContainerAction, ContainerLogLine, ContainersSnapshot } from '@/lib/containers'

export interface ContainersState {
  /** Containers and daemon availability once the first fetch lands. */
  snapshot: ContainersSnapshot | null
  /** True while the first snapshot is loading. */
  loading: boolean
  /** The newest fetch, action, or log problem. */
  error: string | null
  /** The action in flight, as `container:action`, if any. */
  busy: string | null
  /** The selected container's newest log lines. */
  logs: ContainerLogLine[]
  /** True while the log tail is loading. */
  logsLoading: boolean
  /** The newest log problem. */
  logsError: string | null
  /** Fetches containers and availability again. */
  refresh: () => void
  /** Sends one action, then reloads the containers. */
  act: (container: string, action: ContainerAction) => Promise<void>
  /** Loads one container's log tail. */
  loadLogs: (container: string) => Promise<void>
}

const LOG_LINES = 200

/**
 * Reads the container snapshot and runs container actions.
 *
 * There is no polling: the snapshot loads on mount, on demand, and after every
 * action. The log tail loads when the owner selects a container.
 */
export function useContainers(): ContainersState {
  const [snapshot, setSnapshot] = useState<ContainersSnapshot | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState<string | null>(null)
  const [logs, setLogs] = useState<ContainerLogLine[]>([])
  const [logsLoading, setLogsLoading] = useState(false)
  const [logsError, setLogsError] = useState<string | null>(null)
  const [refreshCount, setRefreshCount] = useState(0)
  const newestRequest = useRef(0)
  const newestLogs = useRef(0)

  useEffect(() => {
    const token = refreshCount
    newestRequest.current = token
    let ignore = false
    void (async () => {
      try {
        const { data, error: failure } = await api.GET('/api/containers')
        if (ignore || newestRequest.current !== token) return
        if (data) {
          setSnapshot(data)
          setError(null)
        } else {
          setError(messageOf(failure))
        }
      } catch (cause: unknown) {
        if (ignore || newestRequest.current !== token) return
        setError(cause instanceof Error ? cause.message : 'cannot reach the server')
      } finally {
        if (!ignore && newestRequest.current === token) setLoading(false)
      }
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
    async (container: string, action: ContainerAction) => {
      setBusy(`${container}:${action}`)
      setError(null)
      try {
        const { error: failure } = await api.POST('/api/containers/action', {
          body: { container, action },
        })
        if (failure !== undefined) {
          setError(messageOf(failure))
          return
        }
        reload()
      } catch (cause: unknown) {
        setError(cause instanceof Error ? cause.message : 'the action failed')
      } finally {
        setBusy(null)
      }
    },
    [reload],
  )

  const loadLogs = useCallback(async (container: string) => {
    const token = ++newestLogs.current
    setLogsLoading(true)
    setLogsError(null)
    try {
      const { data, error: failure } = await api.GET('/api/containers/{container}/logs', {
        params: { path: { container }, query: { tail: LOG_LINES } },
      })
      if (token !== newestLogs.current) return
      if (data) {
        setLogs(data)
      } else {
        setLogs([])
        setLogsError(messageOf(failure))
      }
    } catch (cause: unknown) {
      if (token !== newestLogs.current) return
      setLogs([])
      setLogsError(cause instanceof Error ? cause.message : 'cannot reach the server')
    } finally {
      if (token === newestLogs.current) setLogsLoading(false)
    }
  }, [])

  return {
    snapshot,
    loading,
    error,
    busy,
    logs,
    logsLoading,
    logsError,
    refresh,
    act,
    loadLogs,
  }
}
