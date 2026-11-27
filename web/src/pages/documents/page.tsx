import { lazy } from 'react'
import { BookOpen } from 'lucide-react'
import type { PageDefinition } from '@/app/page-registry'

// The screen loads on first visit; the registry keeps the metadata eager.
const Component = lazy(() =>
  import('./screen').then((module) => ({ default: module.DocumentsPage })),
)

export const page: PageDefinition = {
  path: '/documents',
  title: 'Documents',
  icon: BookOpen,
  order: 6,
  Component,
}
