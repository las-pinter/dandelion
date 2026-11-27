import { lazy } from 'react'
import { LayoutDashboard } from 'lucide-react'
import type { PageDefinition } from '@/app/page-registry'

// The screen loads on first visit; the registry keeps the metadata eager.
const Component = lazy(() =>
  import('./screen').then((module) => ({ default: module.DashboardPage })),
)

export const page: PageDefinition = {
  path: '/dashboard',
  title: 'Dashboard',
  icon: LayoutDashboard,
  order: 0,
  Component,
}
