import { lazy } from 'react'
import { Settings2 } from 'lucide-react'
import type { PageDefinition } from '@/app/page-registry'

// The screen loads on first visit; the registry keeps the metadata eager.
const Component = lazy(() =>
  import('./screen').then((module) => ({ default: module.ServicesPage })),
)

export const page: PageDefinition = {
  path: '/services',
  title: 'Services',
  icon: Settings2,
  order: 3,
  Component,
}
