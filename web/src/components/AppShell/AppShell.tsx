import { NavLink, Outlet } from 'react-router'
import type { PageDefinition } from '@/app/page-registry'
import { cn } from '@/lib/utils'

export interface AppShellProps {
  pages: PageDefinition[]
}

/** Placeholder shell; its look is replaced once the design direction is chosen. */
export function AppShell({ pages }: AppShellProps) {
  return (
    <div className="flex min-h-svh bg-background text-foreground">
      <nav aria-label="Main" className="flex w-56 flex-col gap-1 border-r p-3">
        <span className="px-2 pb-3 font-heading text-lg font-semibold">Dandelion</span>
        {pages.map(({ path, title, icon: Icon }) => (
          <NavLink
            key={path}
            to={path}
            className={({ isActive }) =>
              cn(
                'flex items-center gap-2 rounded-md px-2 py-1.5 text-sm hover:bg-accent',
                isActive && 'bg-accent font-medium',
              )
            }
          >
            <Icon aria-hidden className="size-4" />
            {title}
          </NavLink>
        ))}
      </nav>
      <main className="flex-1 p-6">
        <Outlet />
      </main>
    </div>
  )
}
