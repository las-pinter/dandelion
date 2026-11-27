import { lazy } from 'react'
import { ListTree } from 'lucide-react'
import type { PageDefinition } from '@/app/page-registry'

// The screen loads on first visit; the registry keeps the metadata eager.
const Component = lazy(() =>
  import('./screen').then((module) => ({ default: module.ProcessesPage })),
)

export const page: PageDefinition = {
  path: '/processes',
  title: 'Processes',
  icon: ListTree,
  order: 2,
  Component,
}
