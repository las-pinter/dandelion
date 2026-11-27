import { lazy } from 'react'
import { SquareTerminal } from 'lucide-react'
import type { PageDefinition } from '@/app/page-registry'

// The screen loads on first visit; the registry keeps the metadata eager.
const Component = lazy(() =>
  import('./screen').then((module) => ({ default: module.TerminalPage })),
)

export const page: PageDefinition = {
  path: '/terminal',
  title: 'Terminal',
  icon: SquareTerminal,
  order: 5,
  Component,
}
