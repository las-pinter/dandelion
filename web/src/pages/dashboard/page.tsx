import { LayoutDashboard } from 'lucide-react'
import type { PageDefinition } from '@/app/page-registry'

function DashboardPage() {
  return <h1 className="font-heading text-2xl font-semibold">Dashboard</h1>
}

export const page: PageDefinition = {
  path: '/dashboard',
  title: 'Dashboard',
  icon: LayoutDashboard,
  order: 0,
  Component: DashboardPage,
}
