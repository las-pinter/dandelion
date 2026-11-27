import { lazy } from 'react'
import { KeyRound } from 'lucide-react'
import type { PageDefinition } from '@/app/page-registry'

// The screen loads on first visit; the registry keeps the metadata eager.
const Component = lazy(() => import('./screen').then((module) => ({ default: module.SetupPage })))

export const page: PageDefinition = {
  path: '/setup',
  title: 'Setup',
  icon: KeyRound,
  order: 901,
  standalone: true,
  Component,
}
