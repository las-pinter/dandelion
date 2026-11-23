import { createBrowserRouter, Navigate, type RouteObject } from 'react-router'
import { AppShell } from '@/components/AppShell/AppShell'
import { NotFound } from '@/components/NotFound/NotFound'
import { RequireAuth } from '@/components/RequireAuth/RequireAuth'
import { pages, type PageDefinition } from './page-registry'

export function buildRoutes(registered: PageDefinition[]): RouteObject[] {
  const shellPages = registered.filter((page) => page.standalone !== true)
  const standalonePages = registered.filter((page) => page.standalone === true)
  const [first] = shellPages

  return [
    // Sign in and setup render before a session exists, outside the shell.
    ...standalonePages.map(({ path, Component }) => ({ path, element: <Component /> })),
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
            ? [{ path, element: <Component /> }]
            : [
                { path, element: <Component /> },
                // Nested paths stay inside their page until it declares child routes.
                { path: `${path}/*`, element: <Component /> },
              ],
        ),
        { path: '*', element: <NotFound /> },
      ],
    },
  ]
}

export const router = createBrowserRouter(buildRoutes(pages))
