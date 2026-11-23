import { useCallback } from 'react'
import { api } from '@/api/client'
import { terminalSocketUrl } from '@/lib/terminal'
import { XtermPane } from '@/components/XtermPane/XtermPane'

export interface TerminalViewProps {
  /** The local shell whose socket this view drives. */
  id: string
  /** False while the tab is hidden; becoming visible refits the view. */
  active?: boolean
}

/**
 * The Terminal page's view: xterm over the local shell's socket.
 *
 * The shell ends when its socket does, so it only sends a delete when the
 * socket never opened at all.
 */
export function TerminalView({ id, active }: TerminalViewProps) {
  const onClosedWithoutOpen = useCallback(() => {
    void api.DELETE('/api/terminals/{id}', { params: { path: { id } } })
  }, [id])

  return (
    <XtermPane
      url={terminalSocketUrl(id)}
      active={active}
      onClosedWithoutOpen={onClosedWithoutOpen}
    />
  )
}
