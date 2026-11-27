import { lazy } from 'react'
import { Bot } from 'lucide-react'
import type { PageDefinition } from '@/app/page-registry'

// The screen loads on first visit; the registry keeps the metadata eager.
const Component = lazy(() => import('./screen').then((module) => ({ default: module.AgentsPage })))

export const page: PageDefinition = {
  path: '/agents',
  title: 'Agents',
  icon: Bot,
  order: 1,
  Component,
}
