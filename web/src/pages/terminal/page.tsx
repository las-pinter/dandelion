import { useState } from 'react'
import { SquareTerminal } from 'lucide-react'
import type { PageDefinition } from '@/app/page-registry'
import { Panel } from '@/components/Panel/Panel'
import { TerminalTabs } from '@/components/TerminalTabs/TerminalTabs'
import { Button } from '@/components/ui/button'
import { useTerminals } from '@/hooks/use-terminals'
import { MAX_TERMINALS, type TerminalInfo } from '@/lib/terminal'

export function TerminalPage() {
  const { error, busy, open, close } = useTerminals()
  const [tabs, setTabs] = useState<TerminalInfo[]>([])
  const [activeId, setActiveId] = useState<string | null>(null)
  // Derived during render so a tab that closes is adopted without an effect.
  const active =
    activeId && tabs.some((terminal) => terminal.id === activeId) ? activeId : (tabs[0]?.id ?? null)

  const openTab = async () => {
    const info = await open()
    if (info) {
      setTabs((current) => [...current, info])
      setActiveId(info.id)
    }
  }

  const closeTab = async (id: string) => {
    setTabs((current) => current.filter((terminal) => terminal.id !== id))
    await close(id)
  }

  const atLimit = tabs.length >= MAX_TERMINALS

  return (
    <Panel
      title="Terminal"
      description="A shell on this machine, in a browser tab. Closing a tab ends its shell."
      actions={
        <Button size="sm" onClick={() => void openTab()} disabled={busy || atLimit}>
          New tab
        </Button>
      }
    >
      {error && (
        <p role="alert" className="mb-3 text-sm text-destructive">
          {error}
        </p>
      )}
      {atLimit && (
        <p className="mb-3 text-xs text-muted-foreground">
          At most {MAX_TERMINALS} shells may be open at once. Close one to open another.
        </p>
      )}
      {tabs.length === 0 ? (
        <div className="flex flex-col items-center gap-3 py-12 text-center">
          <SquareTerminal aria-hidden className="size-8 text-muted-foreground" />
          <p className="text-sm text-muted-foreground">No shell is open.</p>
          <Button variant="outline" size="sm" onClick={() => void openTab()} disabled={busy}>
            Open a shell
          </Button>
        </div>
      ) : (
        <TerminalTabs
          terminals={tabs}
          activeId={active}
          onSelect={setActiveId}
          onClose={(id) => void closeTab(id)}
        />
      )}
    </Panel>
  )
}

export const page: PageDefinition = {
  path: '/terminal',
  title: 'Terminal',
  icon: SquareTerminal,
  order: 5,
  Component: TerminalPage,
}
