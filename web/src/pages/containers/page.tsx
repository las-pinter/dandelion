import { lazy } from 'react'
import { Boxes } from 'lucide-react'
import type { PageDefinition } from '@/app/page-registry'

// The screen loads on first visit; the registry keeps the metadata eager.
const Component = lazy(() =>
  import('./screen').then((module) => ({ default: module.ContainersPage })),
)

export const page: PageDefinition = {
  path: '/containers',
  title: 'Containers',
  icon: Boxes,
  order: 4,
  Component,
}
