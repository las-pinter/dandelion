import { useEffect, useRef, useState } from 'react'

const MAX_RETRY_MS = 30_000

export interface HubTopicHandlers {
  /** One event on one of the subscribed topics. */
  onEvent: (topic: string, data: unknown) => void
  /** Called when the socket drops under this listener and opens again. */
  onReconnect?: () => void
}

interface HubEvent {
  topic: string
  data: unknown
}

/** One hook's share of the connection: its topics and its callbacks. */
interface Listener {
  topics: Set<string>
  handlers: () => HubTopicHandlers
  setConnected: (value: boolean) => void
  /** Set when the socket drops under this listener, so the next open refetches. */
  dropped: boolean
}

// One connection for the whole page. Every hook that follows the hub joins it,
// so a page with the host card, the metrics and the agent list still opens a
// single socket; the last one to leave closes it.
let socket: WebSocket | null = null
let timer: number | undefined
let retries = 0
const listeners = new Set<Listener>()

function sendSubscribe(ws: WebSocket, topics: Iterable<string>) {
  ws.send(JSON.stringify({ type: 'subscribe', topics: [...topics] }))
}

function connect() {
  if (socket) return
  const scheme = window.location.protocol === 'https:' ? 'wss' : 'ws'
  const ws = new WebSocket(`${scheme}://${window.location.host}/api/ws`)
  socket = ws

  ws.addEventListener('open', () => {
    if (socket !== ws) return
    retries = 0
    const all = new Set<string>()
    for (const listener of listeners) {
      for (const topic of listener.topics) all.add(topic)
    }
    if (all.size > 0) sendSubscribe(ws, all)
    for (const listener of listeners) {
      listener.setConnected(true)
      if (listener.dropped) {
        listener.dropped = false
        listener.handlers().onReconnect?.()
      }
    }
  })

  ws.addEventListener('message', (event) => {
    if (typeof event.data !== 'string') return
    const message = parseEvent(event.data)
    if (!message) return
    for (const listener of listeners) {
      if (listener.topics.has(message.topic)) {
        listener.handlers().onEvent(message.topic, message.data)
      }
    }
  })

  ws.addEventListener('close', () => {
    if (socket !== ws) return
    socket = null
    for (const listener of listeners) {
      listener.setConnected(false)
      listener.dropped = true
    }
    if (listeners.size === 0) return
    const delay = Math.min(MAX_RETRY_MS, 1_000 * 2 ** retries)
    retries += 1
    timer = window.setTimeout(() => {
      timer = undefined
      connect()
    }, delay)
  })

  ws.addEventListener('error', () => ws.close())
}

function disconnect() {
  window.clearTimeout(timer)
  timer = undefined
  retries = 0
  const ws = socket
  socket = null
  ws?.close()
}

/**
 * Subscribes to hub topics on the page's shared `/api/ws` socket.
 *
 * Returns whether the socket is open. A closed socket reconnects with
 * exponential backoff capped at 30 seconds, and a reconnect tells every
 * follower to refetch so events missed in the gap do not leave stale state.
 */
export function useHubTopics(topics: readonly string[], handlers: HubTopicHandlers): boolean {
  const [connected, setConnected] = useState(false)
  const handlersRef = useRef(handlers)
  const key = topics.join(',')

  useEffect(() => {
    handlersRef.current = handlers
  })

  useEffect(() => {
    const listener: Listener = {
      topics: new Set(key.split(',')),
      handlers: () => handlersRef.current,
      setConnected,
      dropped: false,
    }
    listeners.add(listener)
    if (!socket) {
      connect()
    } else if (socket.readyState === 1) {
      // Joining a live socket: ask for these topics and start already open.
      sendSubscribe(socket, listener.topics)
      listener.setConnected(true)
    }
    return () => {
      listeners.delete(listener)
      if (listeners.size === 0) {
        disconnect()
        return
      }
      // A topic no remaining listener wants stops streaming: a stale
      // high-volume topic could otherwise lag the one shared socket.
      if (socket && socket.readyState === 1) {
        const wanted = new Set<string>()
        for (const other of listeners) {
          for (const topic of other.topics) wanted.add(topic)
        }
        const orphaned = [...listener.topics].filter((topic) => !wanted.has(topic))
        if (orphaned.length > 0) {
          socket.send(JSON.stringify({ type: 'unsubscribe', topics: orphaned }))
        }
      }
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
