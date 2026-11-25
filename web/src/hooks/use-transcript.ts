import { useCallback, useEffect, useRef, useState } from 'react'
import {
  parseFrame,
  transcriptSocketUrl,
  withAppended,
  withEarlier,
  type SubagentInfo,
  type TranscriptRecord,
} from '@/lib/transcript'

const MAX_RETRY_MS = 30_000
/** Records kept from the live tail; "load earlier" pages are added on top. */
const KEEP_RECORDS = 500

export interface TranscriptState {
  /** Records oldest first. */
  records: TranscriptRecord[]
  /** Subagent runs of the main session. */
  subagents: SubagentInfo[]
  /** Whether a page exists before the oldest record. */
  hasEarlier: boolean
  /** True until the first page lands. */
  loading: boolean
  /** The newest socket or protocol problem. */
  error: string | null
  /** Asks for the page before the oldest record. */
  loadEarlier: () => void
}

interface Inner extends TranscriptState {
  /** The socket target this state belongs to. */
  key: string | null
}

function fresh(key: string | null): Inner {
  return {
    key,
    records: [],
    subagents: [],
    hasEarlier: false,
    loading: key !== null,
    error: null,
    loadEarlier: () => undefined,
  }
}

/**
 * Streams one transcript over its socket.
 *
 * A dropped socket reconnects with backoff and reloads the newest page, so a
 * gap never leaves the tab stale. `loadEarlier` is a no-op while the first
 * page or an earlier page is still in flight.
 *
 * More than 500 live records since the last page also reopen the socket: the
 * new page realigns with the server's cursor, so earlier pages loaded by hand
 * are dropped at that point.
 */
export function useTranscript(sessionId: string | null, agent?: string | null): TranscriptState {
  const key = sessionId === null ? null : `${sessionId}:${agent ?? ''}`
  const [state, setState] = useState<Inner>(() => fresh(null))
  const socketRef = useRef<WebSocket | null>(null)
  /** Live-appended records since the last page; earlier pages do not count. */
  const liveRef = useRef(0)
  const pagingRef = useRef(false)
  const fatalRef = useRef(false)

  // A new session starts over; React re-renders at once with the fresh state.
  if (state.key !== key) {
    setState(fresh(key))
  }

  useEffect(() => {
    if (sessionId === null) return

    let stopped = false
    let retries = 0
    let timer: number | undefined
    let active: WebSocket | null = null
    fatalRef.current = false
    liveRef.current = 0
    pagingRef.current = false

    const apply = (patch: (current: Inner) => Partial<Inner>) => {
      setState((current) => ({ ...current, ...patch(current) }))
    }

    const connect = () => {
      const socket = new WebSocket(transcriptSocketUrl(sessionId, agent))
      let opened = false
      active = socket
      socketRef.current = socket
      socket.addEventListener('open', () => {
        opened = true
        retries = 0
      })
      socket.addEventListener('message', (event) => {
        if (stopped) return
        const frame = parseFrame(event.data)
        if (!frame) return
        if (frame.type === 'appended') {
          if (liveRef.current + frame.records.length > KEEP_RECORDS) {
            // Realign with the server's cursor: reconnect for a fresh page.
            socket.close()
            return
          }
          liveRef.current += frame.records.length
          apply((current) => ({ records: withAppended(current.records, frame.records) }))
          return
        }
        if (frame.type === 'page') {
          liveRef.current = 0
          apply(() => ({
            records: frame.records,
            hasEarlier: frame.hasEarlier,
            loading: false,
            error: null,
          }))
          return
        }
        if (frame.type === 'earlier') {
          pagingRef.current = false
          apply((current) => ({
            records: withEarlier(current.records, frame.records),
            hasEarlier: frame.hasEarlier,
          }))
          return
        }
        if (frame.type === 'subagents') {
          apply(() => ({ subagents: frame.items }))
          return
        }
        if (frame.type === 'error') {
          fatalRef.current = true
          apply(() => ({ error: frame.message, loading: false }))
        }
      })
      socket.addEventListener('close', () => {
        // A late close from an old socket must not clobber its successor.
        if (socketRef.current === socket) {
          socketRef.current = null
          pagingRef.current = false
        }
        if (stopped || fatalRef.current || active !== socket) return
        if (!opened) {
          // A handshake that never opened must not look like it is still loading.
          apply(() => ({
            loading: false,
            error: 'Cannot reach the transcript socket; retrying.',
          }))
        }
        const wait = Math.min(MAX_RETRY_MS, 1000 * 2 ** retries)
        retries += 1
        timer = window.setTimeout(connect, wait)
      })
    }

    connect()
    return () => {
      stopped = true
      if (timer !== undefined) window.clearTimeout(timer)
      if (socketRef.current === active) socketRef.current = null
      active?.close()
      active = null
    }
  }, [sessionId, agent])

  const loadEarlier = useCallback(() => {
    const socket = socketRef.current
    if (!socket || socket.readyState !== 1 || pagingRef.current) return
    pagingRef.current = true
    socket.send(JSON.stringify({ type: 'earlier' }))
  }, [])

  return { ...state, loadEarlier }
}
