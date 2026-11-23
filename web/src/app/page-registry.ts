import type { ComponentType } from 'react'
import type { LucideIcon } from 'lucide-react'
import type { PaletteCommand } from './command-registry'

/**
 * Every screen lives in `src/pages/<screen>/page.tsx` and exports `page`.
 * The registry discovers them, so adding a screen never edits a shared file.
 */
export interface PageDefinition {
  /** Route path, absolute, e.g. `/agents`. */
  path: string
  /** Label shown in navigation and the document title. */
  title: string
  icon: LucideIcon
  /** Navigation position; lower comes first. */
  order: number
  /** Optional count or short text shown beside the navigation label. */
  badge?: string
  /** Extra command-palette entries this page contributes. */
  commands?: PaletteCommand[]
  /** Render outside the app shell, with no navigation (sign in, setup). */
  standalone?: boolean
  Component: ComponentType
}

export interface PageModule {
  page: PageDefinition
}

export class PageRegistryError extends Error {}

export function buildPageRegistry(modules: Record<string, PageModule>): PageDefinition[] {
  const seen = new Map<string, string>()
  for (const [file, mod] of Object.entries(modules)) {
    if (!mod.page) {
      throw new PageRegistryError(`${file} does not export \`page\``)
    }
    if (!mod.page.path.startsWith('/')) {
      throw new PageRegistryError(`${file}: path "${mod.page.path}" must start with "/"`)
    }
    const other = seen.get(mod.page.path)
    if (other) {
      throw new PageRegistryError(`${file} and ${other} both use path "${mod.page.path}"`)
    }
    seen.set(mod.page.path, file)
  }
  return Object.values(modules)
    .map((mod) => mod.page)
    .sort((a, b) => a.order - b.order || a.title.localeCompare(b.title))
}

/** The page that owns a pathname; the longest matching path wins for nested routes. */
export function findPage(
  registered: PageDefinition[],
  pathname: string,
): PageDefinition | undefined {
  let match: PageDefinition | undefined
  for (const page of registered) {
    const owns =
      page.path === '/'
        ? pathname === '/'
        : pathname === page.path || pathname.startsWith(`${page.path}/`)
    if (owns && (!match || page.path.length > match.path.length)) {
      match = page
    }
  }
  return match
}

export const pages: PageDefinition[] = buildPageRegistry(
  import.meta.glob<PageModule>('../pages/*/page.tsx', { eager: true }),
)
