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
    // A read-only pane follows the stream's own terminal size; this is it.
    let source: { cols: number; rows: number } | null = null
    let follow = true
    // The callback as it was when this socket opened: a late close must not
    // call the successor's callback (which may act on the new target).
    const notify = closedWithoutOpen.current

    const onScroll = () => {
      follow = element.scrollHeight - element.scrollTop - element.clientHeight < 24
    }
    element.addEventListener('scroll', onScroll)

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

      const pin = () => {
        if (follow) element.scrollTop = element.scrollHeight
      }

      // The observe stream is the agent's own screen, drawn for the agent's
      // own width: cursor addressing only lands correctly at that size. Scale
      // the font so those columns fit the pane (never below a readable size;
      // any remaining columns fall outside the view), then keep the live
      // bottom in view instead of squeezing rows into a height that cannot
      // hold them.
      const adopt = (cols: number, rows: number) => {
        if (cols <= 0 || rows <= 0) return
        source = { cols, rows }
        let fontSize = view.options.fontSize || 13
        for (let pass = 0; pass < 3; pass += 1) {
          view.options.fontSize = fontSize
          const proposed = fit.proposeDimensions()
          if (!proposed || proposed.cols === cols) break
          // Columns per font size are inverse proportional: a smaller font
          // fits more of them.
          // Below this the glyphs are unreadable, so the pane keeps the text
          // legible and lets the right-hand columns fall outside the view.
          fontSize = Math.min(24, Math.max(8, (fontSize * proposed.cols) / Math.max(1, cols)))
        }
        view.options.fontSize = fontSize
        if (view.cols !== cols || view.rows !== rows) view.resize(cols, rows)
        pin()
      }

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
          view.write(new Uint8Array(event.data), pin)
          return
        }
        if (typeof event.data !== 'string') return
        const frame = parseFrame(event.data)
        if (!frame) return
        if (frame.type === 'error') failure = frame.message ?? failure
        if (frame.type === 'closed') failure = frame.reason ?? failure
        if (frame.type === 'resized' && readOnly && frame.cols && frame.rows) {
          adopt(frame.cols, frame.rows)
        }
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
        if (readOnly && source) {
          adopt(source.cols, source.rows)
          return
        }
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
      element.removeEventListener('scroll', onScroll)
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

  return (
    <div
      ref={host}
      className={readOnly ? 'h-full w-full overflow-auto' : 'h-full w-full'}
      data-testid="xterm-pane"
    />
  )
}

/** One of the server's text frames: a size, or why the stream stopped. */
interface PaneFrame {
  type: string
  cols?: number
  rows?: number
  message?: string
  reason?: string
}

/** Parses a text frame, dropping anything that is not the expected shape. */
function parseFrame(text: string): PaneFrame | null {
  try {
    const value: unknown = JSON.parse(text)
    if (typeof value !== 'object' || value === null || !('type' in value)) return null
    const frame = value as Record<string, unknown>
    if (typeof frame.type !== 'string') return null
    return {
      type: frame.type,
      cols: typeof frame.cols === 'number' ? frame.cols : undefined,
      rows: typeof frame.rows === 'number' ? frame.rows : undefined,
      message: typeof frame.message === 'string' ? frame.message : undefined,
      reason: typeof frame.reason === 'string' ? frame.reason : undefined,
    }
  } catch {
    return null
  }
}
