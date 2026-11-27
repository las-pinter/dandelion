import { useCallback, useEffect, useRef, useState } from 'react'
import { api } from '@/api/client'
import { messageOf } from '@/lib/api-error'
import type { DocumentsSnapshot } from '@/lib/documents'

export interface DocumentsState {
  /** Collections and manifest warnings once the first fetch lands. */
  snapshot: DocumentsSnapshot | null
  /** True while the first snapshot is loading. */
  loading: boolean
  /** The newest fetch problem. */
  error: string | null
  /** Fetches manifests and their documents again. */
  refresh: () => void
}

/**
 * Reads the document snapshot.
 *
 * No watcher and no polling: manifests and files are listed when the page
 * opens and on the Refresh button. Whatever an agent wrote since then shows
 * up on the next refresh.
 */
export function useDocuments(): DocumentsState {
  const [snapshot, setSnapshot] = useState<DocumentsSnapshot | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [refreshCount, setRefreshCount] = useState(0)
  const newestRequest = useRef(0)

  useEffect(() => {
    const token = refreshCount
    newestRequest.current = token
    let ignore = false
    void (async () => {
      try {
        const { data, error: failure } = await api.GET('/api/documents')
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

  const refresh = useCallback(() => {
    setLoading(true)
    setRefreshCount((count) => count + 1)
  }, [])

  return { snapshot, loading, error, refresh }
}
