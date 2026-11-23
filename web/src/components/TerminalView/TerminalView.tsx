import { useEffect, useRef } from 'react'
import type { FitAddon } from '@xterm/addon-fit'
import type { Terminal } from '@xterm/xterm'
import '@xterm/xterm/css/xterm.css'
import { api } from '@/api/client'
import { resizeMessage, terminalSocketUrl } from '@/lib/terminal'

export interface TerminalViewProps {
  /** The terminal whose socket this view drives. */
  id: string
  /** False while the tab is hidden; becoming visible refits the view. */
  active?: boolean
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
 * One xterm instance bound to one terminal socket.
 *
 * xterm loads on demand, so no other screen pays for it. Binary frames are raw
 * terminal bytes in both directions; the view asks the server for a size on
 * connect and on every resize. The shell ends when the socket does, so it
 * only sends a delete when the socket never opened at all.
 */
export function TerminalView({ id, active = true }: TerminalViewProps) {
  const host = useRef<HTMLDivElement>(null)
  const fitter = useRef<FitAddon | null>(null)

  useEffect(() => {
    const element = host.current
    if (!element) return
    let disposed = false
    let terminal: Terminal | null = null
    let socket: WebSocket | null = null
    let observer: ResizeObserver | null = null
    let input: { dispose: () => void } | null = null
    let opened = false

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

      const ws = new WebSocket(terminalSocketUrl(id))
      socket = ws
      ws.binaryType = 'arraybuffer'

      ws.addEventListener('open', () => {
        opened = true
        ws.send(resizeMessage(view.cols, view.rows))
      })
      ws.addEventListener('message', (event) => {
        if (disposed || !(event.data instanceof ArrayBuffer)) return
        view.write(new Uint8Array(event.data))
      })
      ws.addEventListener('close', () => {
        if (!opened) {
          // A shell whose socket never opened would outlive the page.
          void api.DELETE('/api/terminals/{id}', { params: { path: { id } } })
          return
        }
        if (!disposed) view.write('\r\n[connection closed]\r\n')
      })

      input = view.onData((data) => {
        if (ws.readyState === WebSocket.OPEN) ws.send(new TextEncoder().encode(data))
      })

      observer = new ResizeObserver(() => {
        if (element.offsetParent === null) return
        fit.fit()
        if (ws.readyState === WebSocket.OPEN) ws.send(resizeMessage(view.cols, view.rows))
      })
      observer.observe(element)
    }

    // One tick late, so React's development double-invoke mounts the socket
    // once instead of opening and killing a shell in the same commit.
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
  }, [id])

  useEffect(() => {
    if (active) fitter.current?.fit()
  }, [active])

  return <div ref={host} className="h-full w-full" data-testid="terminal-view" />
}
