import type { LucideIcon } from 'lucide-react'

/**
 * A command the palette can run. Pages contribute commands through their
 * `PageDefinition`, so the palette never needs a central list.
 */
export interface PaletteCommand {
  /** Stable identifier, unique across the palette. */
  id: string
  /** Text shown in the palette and matched by the search box. */
  label: string
  /** Optional group heading; page navigation commands use `Pages`. */
  group?: string
  /** Extra words the search box matches. */
  keywords?: string[]
  icon?: LucideIcon
  /** Navigate to this route when chosen. */
  to?: string
  /** Run this action when chosen. Ignored when `to` is set. */
  run?: () => void
}

/** The shape a page needs to expose to feed the palette. */
export interface CommandSource {
  path: string
  title: string
  icon?: LucideIcon
  commands?: PaletteCommand[]
}

export const PAGE_COMMAND_GROUP = 'Pages'
export const ACTION_COMMAND_GROUP = 'Actions'

export interface CommandGroup {
  name: string
  items: PaletteCommand[]
}

/** One navigation command per page, then every command the page contributes. */
export function buildPaletteCommands(sources: CommandSource[]): PaletteCommand[] {
  return sources.flatMap((source) => [
    {
      id: `page:${source.path}`,
      label: `Go to ${source.title}`,
      group: PAGE_COMMAND_GROUP,
      keywords: [source.title, source.path],
      icon: source.icon,
      to: source.path,
    },
    ...(source.commands ?? []),
  ])
}

/** Keep first-seen group order so pages stay in registry order. */
export function groupCommands(commands: PaletteCommand[]): CommandGroup[] {
  const groups: CommandGroup[] = []
  const byName = new Map<string, CommandGroup>()
  for (const command of commands) {
    const name = command.group ?? ACTION_COMMAND_GROUP
    let group = byName.get(name)
    if (!group) {
      group = { name, items: [] }
      byName.set(name, group)
      groups.push(group)
    }
    group.items.push(command)
  }
  return groups
}
