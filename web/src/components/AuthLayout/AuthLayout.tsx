import type { ReactNode } from 'react'

export interface AuthLayoutProps {
  title: string
  description?: string
  children: ReactNode
}

/** Full-page frame for the screens that exist before a session does. */
export function AuthLayout({ title, description, children }: AuthLayoutProps) {
  return (
    <div className="flex min-h-svh items-center justify-center bg-background p-4 text-foreground">
      <main className="w-full max-w-sm rounded-xl border bg-card p-6">
        <div className="mb-6 flex items-center gap-2">
          <span aria-hidden className="size-2.5 rounded-sm bg-primary" />
          <span className="font-heading text-lg font-semibold tracking-tight">Dandelion</span>
        </div>
        <h1 className="font-heading text-xl font-semibold">{title}</h1>
        {description !== undefined && (
          <p className="mt-1 text-sm text-muted-foreground">{description}</p>
        )}
        <div className="mt-6">{children}</div>
      </main>
    </div>
  )
}
