import { useCallback, useEffect, useRef, useState } from 'react'
import { api } from '@/api/client'
import type { JournalEntry, ServicesSnapshot, UnitAction, UnitScope } from '@/lib/services'

export interface ServicesState {
  /** Units and allowlist rules once the first fetch lands. */
  snapshot: ServicesSnapshot | null
  /** True while the first snapshot is loading. */
  loading: boolean
  /** The newest fetch, action, or journal problem. */
  error: string | null
  /** The action in flight, as `scope:unit:action`, if any. */
  busy: string | null
  /** The selected unit's newest journal entries. */
  journal: JournalEntry[]
  /** True while the journal tail is loading. */
  journalLoading: boolean
  /** The newest journal problem. */
  journalError: string | null
  /** Fetches units and allowlist rules again. */
  refresh: () => void
  /** Sends one action, then reloads the units. */
  act: (scope: UnitScope, unit: string, action: UnitAction) => Promise<void>
  /** Loads one unit's journal tail. */
  loadJournal: (scope: UnitScope, unit: string) => Promise<void>
}

const JOURNAL_LINES = 200

/** The API error message, or a generic fallback. */
function messageOf(failure: unknown): string {
  if (
    typeof failure === 'object' &&
    failure !== null &&
    'message' in failure &&
    typeof failure.message === 'string'
  ) {
    return failure.message
  }
  return 'the request failed'
}

/**
 * Reads the unit snapshot and runs service actions.
 *
 * There is no polling: the snapshot loads on mount, on demand, and after every
 * action. The journal tail loads when the owner selects a unit.
 */
export function useServices(): ServicesState {
  const [snapshot, setSnapshot] = useState<ServicesSnapshot | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState<string | null>(null)
  const [journal, setJournal] = useState<JournalEntry[]>([])
  const [journalLoading, setJournalLoading] = useState(false)
  const [journalError, setJournalError] = useState<string | null>(null)
  const [refreshCount, setRefreshCount] = useState(0)
  const newestRequest = useRef(0)
  const newestJournal = useRef(0)

  useEffect(() => {
    const token = refreshCount
    newestRequest.current = token
    let ignore = false
    void (async () => {
      try {
        const { data, error: failure } = await api.GET('/api/services')
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
    async (scope: UnitScope, unit: string, action: UnitAction) => {
      setBusy(`${scope}:${unit}:${action}`)
      setError(null)
      try {
        const { error: failure } = await api.POST('/api/services/action', {
          body: { scope, unit, action },
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

  const loadJournal = useCallback(async (scope: UnitScope, unit: string) => {
    const token = ++newestJournal.current
    setJournalLoading(true)
    setJournalError(null)
    try {
      const { data, error: failure } = await api.GET('/api/services/{scope}/{unit}/journal', {
        params: { path: { scope, unit }, query: { lines: JOURNAL_LINES } },
      })
      if (token !== newestJournal.current) return
      if (data) {
        setJournal(data)
      } else {
        setJournal([])
        setJournalError(messageOf(failure))
      }
    } catch (cause: unknown) {
      if (token !== newestJournal.current) return
      setJournal([])
      setJournalError(cause instanceof Error ? cause.message : 'cannot reach the server')
    } finally {
      if (token === newestJournal.current) setJournalLoading(false)
    }
  }, [])

  return {
    snapshot,
    loading,
    error,
    busy,
    journal,
    journalLoading,
    journalError,
    refresh,
    act,
    loadJournal,
  }
}
