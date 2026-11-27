import { useEffect, useState } from 'react'
import { Outlet, useLocation } from 'react-router'
import { findPage, type PageDefinition } from '@/app/page-registry'
import { CommandPalette } from '@/components/CommandPalette/CommandPalette'
import type { LiveSampleState } from '@/hooks/use-live-sample'
import { AppShellSidebar } from './AppShellSidebar'
import { AppShellTopBar } from './AppShellTopBar'
import { MobileTabBar } from './MobileTabBar'

export interface AppShellProps {
  pages: PageDefinition[]
  /** A fixed host reading; stories pass one, the app streams it. */
  host?: LiveSampleState
}

/** The frame every page lives in: sidebar, top bar, mobile tabs, palette. */
export function AppShell({ pages, host }: AppShellProps) {
  const [paletteOpen, setPaletteOpen] = useState(false)
  const { pathname } = useLocation()
  const active = findPage(pages, pathname)

  useEffect(() => {
    document.title = active ? `${active.title} · Dandelion` : 'Dandelion'
  }, [active])

  return (
    <div className="flex min-h-svh bg-background text-foreground">
      <a
        href="#main"
        className="sr-only focus:not-sr-only focus:absolute focus:left-4 focus:top-4 focus:z-50 focus:rounded-lg focus:bg-card focus:px-3 focus:py-2 focus:text-sm focus:text-card-foreground focus:ring-2 focus:ring-ring"
      >
        Skip to content
      </a>
      <AppShellSidebar pages={pages} host={host} />
      <div className="flex min-w-0 flex-1 flex-col">
        <AppShellTopBar
          title={active?.title ?? 'Dandelion'}
          isPage={Boolean(active)}
          onOpenPalette={() => setPaletteOpen(true)}
        />
        <main id="main" tabIndex={-1} className="min-w-0 flex-1 p-4 pb-24 outline-none md:p-6">
          <Outlet />
        </main>
      </div>
      <MobileTabBar pages={pages} />
      <CommandPalette pages={pages} open={paletteOpen} onOpenChange={setPaletteOpen} />
    </div>
  )
}
