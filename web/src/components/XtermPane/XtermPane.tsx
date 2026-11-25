import { useEffect, useRef } from 'react'
import type { FitAddon } from '@xterm/addon-fit'
import type { Terminal } from '@xterm/xterm'
import '@xterm/xterm/css/xterm.css'
import { resizeMessage } from '@/lib/terminal'

export interface XtermPaneProps {
  /** The socket that carries raw terminal bytes. */
  url: string
  /** False while the pane is hidden; becoming visible refits it. */
  active?: boolean
  /**
   * Watch only: keystrokes and resizes stay in the browser. Use this whenever
   * the terminal belongs to someone else.
   */
  readOnly?: boolean
  /** Called when the socket ends without ever opening, with the server's reason. */
  onClosedWithoutOpen?: (reason?: string) => void
}

/** Reads the theme colors xterm cannot inherit from CSS. */
function themeOf(element: HTMLElement) {
  const styles = getComputedStyle(element)
  const color = (name: string, fallback: string) => styles.getPropertyValue(name).trim() || fallback
  return {
    background: color('--background', '#07090d'),
    foreground: color('--foreground', '#e8ecf2'),
    cursor: color('--primary', '#d9f75a'),
    cursorAccent: color('--background', '#07090d'),
    selectionBackground: color('--secondary', '#1a2028'),
  }
}

/**
 * One xterm instance bound to one binary socket.
 *
 * xterm loads on demand. Binary frames are raw terminal bytes in both
 * directions; the pane asks the server for a size on connect and on every
 * resize, unless it is read-only. A text frame the server sends before a
 * failed open is kept as the reason. Unmounting only closes the socket.
 */
export function XtermPane({
  url,
  active = true,
  readOnly = false,
  onClosedWithoutOpen,
}: XtermPaneProps) {
  const host = useRef<HTMLDivElement>(null)
  const fitter = useRef<FitAddon | null>(null)
  // The callback is read through a ref: a parent that re-renders (typing in
  // its prompt box does) passes a new function each time, and that must not
  // reopen the socket and repaint the terminal.
  const closedWithoutOpen = useRef(onClosedWithoutOpen)

  useEffect(() => {
    closedWithoutOpen.current = onClosedWithoutOpen
  }, [onClosedWithoutOpen])

  useEffect(() => {
    const element = host.current
    if (!element) return
    let disposed = false
    let terminal: Terminal | null = null
    let socket: WebSocket | null = null
    let observer: ResizeObserver | null = null
    let input: { dispose: () => void } | null = null
    let opened = false
    let failure: string | undefined
    // The callback as it was when this socket opened: a late close must not
    // call the successor's callback (which may act on the new target).
    const notify = closedWithoutOpen.current

    const start = async () => {
      const [{ Terminal: Xterm }, { FitAddon: Fit }] = await Promise.all([
        import('@xterm/xterm'),
        import('@xterm/addon-fit'),
      ])
      if (disposed) return

      const view = new Xterm({
        convertEol: true,
        cursorBlink: true,
        fontSize: 13,
        scrollback: 5000,
        theme: themeOf(element),
      })
      terminal = view
      const fit = new Fit()
      fitter.current = fit
      view.loadAddon(fit)
      view.open(element)
      fit.fit()

      const ws = new WebSocket(url)
      socket = ws
      ws.binaryType = 'arraybuffer'

      ws.addEventListener('open', () => {
        if (disposed) return
        opened = true
        if (!readOnly) ws.send(resizeMessage(view.cols, view.rows))
      })
      ws.addEventListener('message', (event) => {
        if (disposed) return
        if (event.data instanceof ArrayBuffer) {
          view.write(new Uint8Array(event.data))
          return
        }
        if (typeof event.data === 'string') failure = textStatus(event.data) ?? failure
      })
      ws.addEventListener('close', () => {
        if (!opened) {
          notify?.(failure)
          return
        }
        if (!disposed) view.write('\r\n[connection closed]\r\n')
      })

      if (!readOnly) {
        input = view.onData((data) => {
          if (ws.readyState === WebSocket.OPEN) ws.send(new TextEncoder().encode(data))
        })
      }

      observer = new ResizeObserver(() => {
        if (element.offsetParent === null) return
        fit.fit()
        if (readOnly || ws.readyState !== WebSocket.OPEN) return
        ws.send(resizeMessage(view.cols, view.rows))
      })
      observer.observe(element)
    }

    // One tick late, so React's development double-invoke mounts the socket
    // once instead of opening and closing it in the same commit.
    const timer = setTimeout(() => void start(), 0)

    return () => {
      disposed = true
      clearTimeout(timer)
      observer?.disconnect()
      input?.dispose()
      socket?.close()
      terminal?.dispose()
      fitter.current = null
    }
  }, [url, readOnly])

  useEffect(() => {
    if (active) fitter.current?.fit()
  }, [active])

  return <div ref={host} className="h-full w-full" data-testid="xterm-pane" />
}

/** The message in one of the server's text frames, when it explains a stop. */
function textStatus(text: string): string | undefined {
  try {
    const value: unknown = JSON.parse(text)
    if (typeof value !== 'object' || value === null || !('type' in value)) return undefined
    if (value.type === 'error' && 'message' in value && typeof value.message === 'string') {
      return value.message
    }
    if (value.type === 'closed' && 'reason' in value && typeof value.reason === 'string') {
      return value.reason
    }
    return undefined
  } catch {
    return undefined
  }
}
