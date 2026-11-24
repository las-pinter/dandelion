import { useEffect, useRef, useState } from 'react'

const MAX_RETRY_MS = 30_000

export interface HubTopicHandlers {
  /** One event on one of the subscribed topics. */
  onEvent: (topic: string, data: unknown) => void
  /** Called when a socket that had opened connects again. */
  onReconnect?: () => void
}

interface HubEvent {
  topic: string
  data: unknown
}

/**
 * Subscribes to hub topics on the shared `/api/ws` socket.
 *
 * Returns whether the socket is open. A closed socket reconnects with
 * exponential backoff capped at 30 seconds, and a reconnect tells the caller
 * to refetch so events missed in the gap do not leave stale state.
 */
export function useHubTopics(topics: readonly string[], handlers: HubTopicHandlers): boolean {
  const [connected, setConnected] = useState(false)
  const handlersRef = useRef(handlers)
  const key = topics.join(',')

  useEffect(() => {
    handlersRef.current = handlers
  })

  useEffect(() => {
    const subscribed = new Set(key.split(','))
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
        socket?.send(JSON.stringify({ type: 'subscribe', topics: [...subscribed] }))
        if (reconnected) handlersRef.current.onReconnect?.()
      })
      socket.addEventListener('message', (event) => {
        if (typeof event.data !== 'string') return
        const message = parseEvent(event.data)
        if (message && subscribed.has(message.topic)) {
          handlersRef.current.onEvent(message.topic, message.data)
        }
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
  }, [key])

  return connected
}

function parseEvent(text: string): HubEvent | null {
  try {
    const value: unknown = JSON.parse(text)
    if (typeof value !== 'object' || value === null) return null
    if (!('type' in value) || value.type !== 'event') return null
    if (!('topic' in value) || typeof value.topic !== 'string') return null
    return { topic: value.topic, data: 'data' in value ? value.data : undefined }
  } catch {
    return null
  }
}
