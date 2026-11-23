import { useCallback, useEffect, useRef, useState } from 'react'
import { api } from '@/api/client'
import { messageOf } from '@/lib/api-error'
import type { AgentAction, AgentInfo, AgentsSnapshot, SpawnAgentRequest } from '@/lib/agents'
import { useHubTopics } from './use-hub-topics'

const TOPIC = 'agents'

export interface AgentsState {
  /** Agents and the backend descriptor once the first fetch lands. */
  snapshot: AgentsSnapshot | null
  /** True while the first snapshot loads. */
  loading: boolean
  /** The newest fetch, action, or spawn problem. */
  error: string | null
  /** The action in flight, as `terminal:kind`, if any. */
  busy: string | null
  /** Fetches the snapshot again. */
  refresh: () => void
  /** Runs one action, then reloads the snapshot. */
  act: (terminal: string, action: AgentAction) => Promise<void>
  /** Starts an agent, returning its terminal on success. */
  spawn: (request: SpawnAgentRequest) => Promise<AgentInfo | null>
}

/**
 * Reads the agent snapshot, follows the `agents` hub topic, and runs actions.
 *
 * A state event refetches the snapshot: the list is small, and one fetch is
 * simpler than merging the backend's change stream in the browser. A
 * reconnect refetches too, so a state change during the gap is not missed.
 */
export function useAgents(): AgentsState {
  const [snapshot, setSnapshot] = useState<AgentsSnapshot | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState<string | null>(null)
  const [refreshCount, setRefreshCount] = useState(0)
  const newestRequest = useRef(0)

  useEffect(() => {
    const token = refreshCount
    newestRequest.current = token
    let ignore = false
    void (async () => {
      try {
        const { data, error: failure } = await api.GET('/api/agents')
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

  useHubTopics([TOPIC], {
    onEvent: () => reload(),
    onReconnect: reload,
  })

  const refresh = useCallback(() => {
    setLoading(true)
    reload()
  }, [reload])

  const act = useCallback(
    async (terminal: string, action: AgentAction) => {
      setBusy(`${terminal}:${action.kind}`)
      setError(null)
      try {
        const { error: failure } = await api.POST('/api/agents/action', {
          body: { terminal, action },
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

  const spawn = useCallback(
    async (request: SpawnAgentRequest) => {
      setError(null)
      try {
        const { data, error: failure } = await api.POST('/api/agents/spawn', { body: request })
        if (data) {
          reload()
          return data
        }
        setError(messageOf(failure))
        return null
      } catch (cause: unknown) {
        setError(cause instanceof Error ? cause.message : 'the agent could not start')
        return null
      }
    },
    [reload],
  )

  return { snapshot, loading, error, busy, refresh, act, spawn }
}
