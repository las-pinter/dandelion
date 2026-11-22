import { useEffect, useMemo } from 'react'
import { useNavigate } from 'react-router'
import { buildPaletteCommands, groupCommands, type PaletteCommand } from '@/app/command-registry'
import type { PageDefinition } from '@/app/page-registry'
import {
  Command,
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandShortcut,
} from '@/components/ui/command'

export interface CommandPaletteProps {
  pages: PageDefinition[]
  open: boolean
  onOpenChange: (open: boolean) => void
}

/** ⌘K / Ctrl+K palette over every page and every command a page contributes. */
export function CommandPalette({ pages, open, onOpenChange }: CommandPaletteProps) {
  const navigate = useNavigate()
  const groups = useMemo(() => groupCommands(buildPaletteCommands(pages)), [pages])

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (event.key.toLowerCase() === 'k' && (event.metaKey || event.ctrlKey)) {
        event.preventDefault()
        onOpenChange(!open)
      }
    }
    document.addEventListener('keydown', onKeyDown)
    return () => document.removeEventListener('keydown', onKeyDown)
  }, [open, onOpenChange])

  function choose(command: PaletteCommand) {
    onOpenChange(false)
    if (command.to) navigate(command.to)
    else command.run?.()
  }

  return (
    <CommandDialog
      open={open}
      onOpenChange={onOpenChange}
      title="Command palette"
      description="Search pages and actions"
    >
      <Command>
        <CommandInput placeholder="Search pages and actions…" />
        <CommandList>
          <CommandEmpty>No matching command.</CommandEmpty>
          {groups.map((group) => (
            <CommandGroup key={group.name} heading={group.name}>
              {group.items.map((command) => {
                const Icon = command.icon
                return (
                  <CommandItem
                    key={command.id}
                    value={[command.label, ...(command.keywords ?? [])].join(' ')}
                    onSelect={() => choose(command)}
                    className="min-h-11"
                  >
                    {Icon && <Icon aria-hidden />}
                    <span>{command.label}</span>
                    {command.to && <CommandShortcut>{command.to}</CommandShortcut>}
                  </CommandItem>
                )
              })}
            </CommandGroup>
          ))}
        </CommandList>
      </Command>
    </CommandDialog>
  )
}
