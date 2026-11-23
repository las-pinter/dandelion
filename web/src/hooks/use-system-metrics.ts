import { useEffect, useMemo, useState } from 'react'
import { api } from '@/api/client'
import type { ServerMessage } from '@/api/gen/ws/ServerMessage'
import {
  HOUR_MS,
  isLiveSample,
  trimToWindow,
  withLivePoint,
  type MetricsPoint,
  type MetricsRange,
  type MinuteAggregate,
  type Snapshot,
} from '@/lib/metrics'

const TOPIC = 'system.metrics'
const MAX_RETRY_MS = 30_000
const REFRESH_MS = 5 * 60_000

export interface SystemMetricsState {
  /** The newest full snapshot, once the socket or the API delivers one. */
  snapshot: Snapshot | null
  /** Second points for the last hour, with the live sample appended. */
  seconds: MetricsPoint[]
  /** Minute aggregates for the last week. */
  minutes: MinuteAggregate[]
  /** Whether the WebSocket is open. */
  connected: boolean
  /** A fetch problem, if any. */
  error: string | null
}

/**
 * Loads stored history for the selected window and subscribes to live samples.
 *
 * History comes from the REST API; the socket pushes a full snapshot and its
 * compact point every second. A closed socket reconnects with exponential
 * backoff and refetches history, and history is reconciled every five minutes
 * so the live 1 h series stays inside its window.
 */
export function useSystemMetrics(range: MetricsRange): SystemMetricsState {
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null)
  const [live, setLive] = useState<MetricsPoint | null>(null)
  const [seconds, setSeconds] = useState<MetricsPoint[]>([])
  const [minutes, setMinutes] = useState<MinuteAggregate[]>([])
  const [connected, setConnected] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [refresh, setRefresh] = useState(0)
  const request = useMemo(() => ({ range, refresh }), [range, refresh])

  useEffect(() => {
    let ignore = false
    void (async () => {
      const [latest, history] = await Promise.all([
        api.GET('/api/system/latest'),
        api.GET('/api/system/history', { params: { query: { range: request.range } } }),
      ])
      if (ignore) return
      const latestSnapshot = latest.data
      if (latestSnapshot) {
        setSnapshot((previous) =>
          !previous || latestSnapshot.timestamp_ms >= previous.timestamp_ms
            ? latestSnapshot
            : previous,
        )
      }
      if (history.data) {
        setSeconds(history.data.seconds)
        setMinutes(history.data.minutes)
        setError(null)
      } else {
        setError('history unavailable')
      }
    })().catch((cause: unknown) => {
      if (!ignore) {
        setError(cause instanceof Error ? cause.message : 'cannot reach the server')
      }
    })
    return () => {
      ignore = true
    }
  }, [request])

  useEffect(() => {
    const timer = window.setInterval(() => setRefresh((value) => value + 1), REFRESH_MS)
    return () => window.clearInterval(timer)
  }, [])

  useEffect(() => {
    let socket: WebSocket | null = null
    let timer: number | undefined
    let retries = 0
    let stopped = false

    const connect = () => {
      const scheme = window.location.protocol === 'https:' ? 'wss' : 'ws'
      socket = new WebSocket(`${scheme}://${window.location.host}/api/ws`)
      socket.addEventListener('open', () => {
        const reconnected = retries > 0
        retries = 0
        setConnected(true)
        socket?.send(JSON.stringify({ type: 'subscribe', topics: [TOPIC] }))
        if (reconnected) setRefresh((value) => value + 1)
      })
      socket.addEventListener('message', (event) => {
        if (typeof event.data !== 'string') return
        const message = parseServerMessage(event.data)
        if (message?.type !== 'event' || message.topic !== TOPIC) return
        if (!isLiveSample(message.data)) return

        const sample = message.data
        setSnapshot((previous) =>
          !previous || sample.snapshot.timestamp_ms >= previous.timestamp_ms
            ? sample.snapshot
            : previous,
        )
        setLive((previous) =>
          !previous || sample.point.timestamp_ms >= previous.timestamp_ms ? sample.point : previous,
        )
      })
      socket.addEventListener('close', () => {
        setConnected(false)
        socket = null
        if (stopped) return
        const delay = Math.min(MAX_RETRY_MS, 1_000 * 2 ** retries)
        retries += 1
        timer = window.setTimeout(connect, delay)
      })
      socket.addEventListener('error', () => socket?.close())
    }

    connect()
    return () => {
      stopped = true
      window.clearTimeout(timer)
      socket?.close()
    }
  }, [])

  const secondsWithLive = useMemo(() => {
    if (range !== '1h' || !live) return seconds
    return trimToWindow(withLivePoint(seconds, live), live.timestamp_ms, HOUR_MS)
  }, [range, seconds, live])

  return { snapshot, seconds: secondsWithLive, minutes, connected, error }
}

function parseServerMessage(text: string): ServerMessage | null {
  try {
    const value: unknown = JSON.parse(text)
    if (typeof value !== 'object' || value === null || !('type' in value)) return null
    return value as ServerMessage
  } catch {
    return null
  }
}
