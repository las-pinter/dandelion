import { Suspense, type ComponentType } from 'react'
import { createBrowserRouter, Navigate, type RouteObject } from 'react-router'
import { AppShell } from '@/components/AppShell/AppShell'
import { NotFound } from '@/components/NotFound/NotFound'
import { RequireAuth } from '@/components/RequireAuth/RequireAuth'
import { pages, type PageDefinition } from './page-registry'

/** A page module loads on first visit, so every route needs a boundary. */
function route(Component: ComponentType) {
  return (
    <Suspense fallback={<p className="p-4 text-sm text-muted-foreground">Loading the page…</p>}>
      <Component />
    </Suspense>
  )
}

export function buildRoutes(registered: PageDefinition[]): RouteObject[] {
  const shellPages = registered.filter((page) => page.standalone !== true)
  const standalonePages = registered.filter((page) => page.standalone === true)
  const [first] = shellPages

  return [
    // Sign in and setup render before a session exists, outside the shell.
    ...standalonePages.map(({ path, Component }) => ({ path, element: route(Component) })),
    {
      element: (
        <RequireAuth>
          <AppShell pages={shellPages} />
        </RequireAuth>
      ),
      children: [
        // Only redirect when a target exists; redirecting `/` to `/` would loop.
        ...(first ? [{ index: true, element: <Navigate to={first.path} replace /> }] : []),
        ...shellPages.flatMap(({ path, Component }) =>
          path === '/'
            ? [{ path, element: route(Component) }]
            : [
                { path, element: route(Component) },
                // Nested paths stay inside their page until it declares child routes.
                { path: `${path}/*`, element: route(Component) },
              ],
        ),
        { path: '*', element: <NotFound /> },
      ],
    },
  ]
}

export const router = createBrowserRouter(buildRoutes(pages))
