import { lazy } from 'react'
import { LogIn } from 'lucide-react'
import type { PageDefinition } from '@/app/page-registry'

// The screen loads on first visit; the registry keeps the metadata eager.
const Component = lazy(() => import('./screen').then((module) => ({ default: module.LoginPage })))

export const page: PageDefinition = {
  path: '/login',
  title: 'Sign in',
  icon: LogIn,
  order: 900,
  standalone: true,
  Component,
}
