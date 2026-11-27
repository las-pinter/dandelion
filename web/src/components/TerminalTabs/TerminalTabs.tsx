import { X } from 'lucide-react'
import { FOCUS_OUTLINE } from '@/lib/focus'
import { cn } from '@/lib/utils'
import { shellLabel, type TerminalInfo } from '@/lib/terminal'
import { Button } from '@/components/ui/button'
import { TerminalView } from '@/components/TerminalView/TerminalView'

export interface TerminalTabsProps {
  terminals: TerminalInfo[]
  activeId: string | null
  onSelect: (id: string) => void
  onClose: (id: string) => void
}

/** A tab strip over one live xterm view per open shell. */
export function TerminalTabs({ terminals, activeId, onSelect, onClose }: TerminalTabsProps) {
  return (
    <div className="flex h-[65vh] min-h-80 flex-col gap-2">
      <div role="tablist" aria-label="Terminals" className="flex flex-wrap gap-1">
        {terminals.map((terminal, index) => {
          const name = `Terminal ${index + 1}: ${shellLabel(terminal.shell)}`
          const selected = terminal.id === activeId
          return (
            <span
              key={terminal.id}
              className={cn(
                'flex items-center rounded-lg border text-sm',
                selected ? 'border-primary/60 bg-accent' : 'bg-transparent',
              )}
            >
              <button
                type="button"
                role="tab"
                aria-selected={selected}
                aria-label={name}
                className={`px-3 py-1 font-medium ${FOCUS_OUTLINE}`}
                onClick={() => onSelect(terminal.id)}
              >
                {shellLabel(terminal.shell)}
              </button>
              <Button
                type="button"
                variant="quiet"
                size="icon-xs"
                aria-label={`Close ${name}`}
                onClick={() => onClose(terminal.id)}
              >
                <X aria-hidden className="size-3.5" />
              </Button>
            </span>
          )
        })}
      </div>
      <div className="relative min-h-0 flex-1 overflow-hidden rounded-lg border bg-background">
        {terminals.map((terminal) => (
          <div
            key={terminal.id}
            role="tabpanel"
            aria-label={`Terminal ${shellLabel(terminal.shell)}`}
            hidden={terminal.id !== activeId}
            className="absolute inset-0 p-1"
          >
            <TerminalView id={terminal.id} active={terminal.id === activeId} />
          </div>
        ))}
      </div>
    </div>
  )
}
