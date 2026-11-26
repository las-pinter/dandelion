import { NavLink } from 'react-router'
import type { PageDefinition } from '@/app/page-registry'
import { cn } from '@/lib/utils'
import { StatusDot } from '@/components/StatusDot/StatusDot'

export interface AppShellSidebarProps {
  pages: PageDefinition[]
}

function HostCard() {
  return (
    <div className="m-3 rounded-xl border border-sidebar-border bg-card p-3">
      <p className="text-[10px] font-medium uppercase tracking-widest text-muted-foreground">
        Host
      </p>
      <p className="mt-1 truncate font-mono text-sm font-medium">dandelion</p>
      <div className="mt-2 flex flex-wrap items-center justify-between gap-2">
        <StatusDot status="idle" withLabel />
        <span className="font-mono text-xs text-muted-foreground">8 cores · 15.2 GiB</span>
      </div>
    </div>
  )
}

/** Desktop navigation: brand, registry-driven links, and the host card. */
export function AppShellSidebar({ pages }: AppShellSidebarProps) {
  return (
    <aside className="hidden w-64 shrink-0 flex-col border-r border-sidebar-border bg-sidebar text-sidebar-foreground md:flex">
      <div className="flex h-[var(--app-top-bar)] shrink-0 items-center gap-2 border-b border-sidebar-border px-4">
        <span aria-hidden className="size-2.5 rounded-full bg-brand" />
        <span className="font-heading text-base font-semibold tracking-tight">Dandelion</span>
      </div>
      <nav aria-label="Primary" className="flex flex-1 flex-col gap-1 overflow-y-auto p-3">
        {pages.map(({ path, title, icon: Icon, badge }) => (
          <NavLink
            key={path}
            to={path}
            aria-label={badge !== undefined ? `${title}, ${badge} notifications` : undefined}
            className={({ isActive }) =>
              cn(
                'flex min-h-11 items-center gap-3 rounded-xl px-3 text-sm transition-colors duration-150',
                'hover:bg-sidebar-accent hover:text-sidebar-accent-foreground',
                isActive &&
                  'bg-sidebar-accent font-medium text-sidebar-accent-foreground [&_svg]:text-sidebar-primary',
              )
            }
          >
            <Icon aria-hidden className="size-4 shrink-0" />
            <span className="truncate">{title}</span>
            {badge !== undefined && (
              <span
                aria-hidden
                className="ml-auto rounded-full bg-muted px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground"
              >
                {badge}
              </span>
            )}
          </NavLink>
        ))}
      </nav>
      <HostCard />
    </aside>
  )
}
