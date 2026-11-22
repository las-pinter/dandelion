import { createBrowserRouter, Navigate, type RouteObject } from 'react-router'
import { AppShell } from '@/components/AppShell/AppShell'
import { NotFound } from '@/components/NotFound/NotFound'
import { pages, type PageDefinition } from './page-registry'

export function buildRoutes(registered: PageDefinition[]): RouteObject[] {
  const [first] = registered
  return [
    {
      element: <AppShell pages={registered} />,
      children: [
        // Only redirect when a target exists; redirecting `/` to `/` would loop.
        ...(first ? [{ index: true, element: <Navigate to={first.path} replace /> }] : []),
        ...registered.flatMap(({ path, Component }) =>
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
