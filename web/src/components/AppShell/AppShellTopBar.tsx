import { Search } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { ThemeToggle } from '@/components/ThemeToggle/ThemeToggle'

export interface AppShellTopBarProps {
  title: string
  /** False when no page owns the route, so the page body keeps the only h1. */
  isPage: boolean
  onOpenPalette: () => void
}

/** Page title on the left; palette and theme controls on the right. */
export function AppShellTopBar({ title, isPage, onOpenPalette }: AppShellTopBarProps) {
  return (
    <header
      data-app-chrome="top"
      className="sticky top-0 z-30 flex h-[var(--app-top-bar)] shrink-0 items-center gap-2 border-b bg-background/80 px-4 backdrop-blur"
    >
      <div className="min-w-0 flex-1">
        <p className="text-[10px] font-medium uppercase tracking-widest text-muted-foreground md:hidden">
          Dandelion
        </p>
        {isPage ? (
          <h1 className="truncate font-heading text-lg font-semibold">{title}</h1>
        ) : (
          <p className="truncate font-heading text-lg font-semibold">{title}</p>
        )}
      </div>
      <Button
        type="button"
        variant="outline"
        onClick={onOpenPalette}
        aria-label="Open command palette"
        className="h-11 min-w-11 gap-2 px-3 max-md:w-11 max-md:px-0 md:h-9"
      >
        <Search aria-hidden />
        <span className="hidden md:inline">Search</span>
        <kbd className="hidden rounded border bg-muted px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground md:inline">
          ⌘K
        </kbd>
      </Button>
      <ThemeToggle />
    </header>
  )
}
