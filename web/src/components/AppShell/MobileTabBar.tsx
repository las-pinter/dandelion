import { MoreHorizontal } from 'lucide-react'
import { useState } from 'react'
import { NavLink } from 'react-router'
import type { PageDefinition } from '@/app/page-registry'
import { cn } from '@/lib/utils'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'

const MAX_TABS = 4

const tabClass = ({ isActive }: { isActive: boolean }) =>
  cn(
    'flex min-h-12 min-w-11 flex-1 basis-0 flex-col items-center justify-center gap-1 px-1 text-[11px] transition-colors',
    isActive ? 'text-primary' : 'text-muted-foreground',
  )

export interface MobileTabBarProps {
  pages: PageDefinition[]
}

/** Below 768 px the sidebar becomes a bottom tab bar; extra pages open as a sheet. */
export function MobileTabBar({ pages }: MobileTabBarProps) {
  const [showMore, setShowMore] = useState(false)
  const visible = pages.slice(0, MAX_TABS)
  const overflow = pages.slice(MAX_TABS)

  return (
    <>
      <nav
        aria-label="Primary mobile"
        className="fixed inset-x-0 bottom-0 z-40 flex border-t bg-background/95 pb-[env(safe-area-inset-bottom)] backdrop-blur md:hidden"
      >
        {visible.map(({ path, title, icon: Icon }) => (
          <NavLink key={path} to={path} className={tabClass}>
            <Icon aria-hidden className="size-5 shrink-0" />
            <span className="w-full truncate text-center">{title}</span>
          </NavLink>
        ))}
        {overflow.length > 0 && (
          <button
            type="button"
            onClick={() => setShowMore(true)}
            aria-haspopup="dialog"
            aria-expanded={showMore}
            className="flex min-h-12 min-w-11 flex-1 basis-0 flex-col items-center justify-center gap-1 px-1 text-[11px] text-muted-foreground"
          >
            <MoreHorizontal aria-hidden className="size-5 shrink-0" />
            <span className="w-full truncate text-center">More</span>
          </button>
        )}
      </nav>
      <Dialog open={showMore} onOpenChange={setShowMore}>
        <DialogContent
          showCloseButton={false}
          aria-describedby={undefined}
          className="top-auto bottom-0 left-0 max-w-none translate-x-0 translate-y-0 rounded-b-none rounded-t-2xl pb-[calc(env(safe-area-inset-bottom)+1rem)] sm:max-w-none"
        >
          <DialogHeader>
            <DialogTitle>More pages</DialogTitle>
          </DialogHeader>
          <nav aria-label="More pages" className="flex flex-col gap-1">
            {overflow.map(({ path, title, icon: Icon }) => (
              <NavLink
                key={path}
                to={path}
                onClick={() => setShowMore(false)}
                className={({ isActive }) =>
                  cn(
                    'flex min-h-11 items-center gap-3 rounded-xl px-3 text-sm transition-colors',
                    isActive
                      ? 'bg-accent font-medium text-accent-foreground'
                      : 'hover:bg-accent hover:text-accent-foreground',
                  )
                }
              >
                <Icon aria-hidden className="size-4 shrink-0" />
                {title}
              </NavLink>
            ))}
          </nav>
        </DialogContent>
      </Dialog>
    </>
  )
}
