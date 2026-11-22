import { Activity } from 'lucide-react'
import {
  ACTION_COMMAND_GROUP,
  buildPaletteCommands,
  groupCommands,
  PAGE_COMMAND_GROUP,
  type PaletteCommand,
} from './command-registry'

describe('buildPaletteCommands', () => {
  it('builds one navigation command per page in registry order', () => {
    const commands = buildPaletteCommands([
      { path: '/one', title: 'One', icon: Activity },
      { path: '/two', title: 'Two' },
    ])

    expect(commands.map((command) => command.id)).toEqual(['page:/one', 'page:/two'])
    expect(commands[0]).toMatchObject({
      label: 'Go to One',
      group: PAGE_COMMAND_GROUP,
      keywords: ['One', '/one'],
      icon: Activity,
      to: '/one',
    })
  })

  it('keeps commands contributed by a page', () => {
    const run = vi.fn<() => void>()
    const contributed: PaletteCommand = { id: 'one:refresh', label: 'Refresh One', run }

    const commands = buildPaletteCommands([{ path: '/one', title: 'One', commands: [contributed] }])

    expect(commands).toEqual([
      {
        id: 'page:/one',
        label: 'Go to One',
        group: PAGE_COMMAND_GROUP,
        keywords: ['One', '/one'],
        icon: undefined,
        to: '/one',
      },
      contributed,
    ])
  })
})

describe('groupCommands', () => {
  it('groups in first-seen order and defaults ungrouped commands to Actions', () => {
    const commands: PaletteCommand[] = [
      { id: 'page:/one', label: 'Go to One', group: PAGE_COMMAND_GROUP },
      { id: 'one:refresh', label: 'Refresh One' },
      { id: 'page:/two', label: 'Go to Two', group: PAGE_COMMAND_GROUP },
    ]

    const groups = groupCommands(commands)

    expect(groups.map((group) => group.name)).toEqual([PAGE_COMMAND_GROUP, ACTION_COMMAND_GROUP])
    expect(groups[0]?.items.map((item) => item.id)).toEqual(['page:/one', 'page:/two'])
    expect(groups[1]?.items.map((item) => item.id)).toEqual(['one:refresh'])
  })
})
