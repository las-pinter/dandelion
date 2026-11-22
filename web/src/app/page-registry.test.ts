import { LayoutDashboard } from 'lucide-react'
import { buildPageRegistry, findPage, PageRegistryError, type PageModule } from './page-registry'

const Empty = () => null

function pageModule(path: string, title: string, order: number): PageModule {
  return { page: { path, title, order, icon: LayoutDashboard, Component: Empty } }
}

describe('buildPageRegistry', () => {
  it('sorts pages by order, then title', () => {
    const modules = {
      a: pageModule('/b', 'Beta', 2),
      b: pageModule('/a', 'Alpha', 1),
      c: pageModule('/c', 'Aardvark', 2),
    }

    const titles = buildPageRegistry(modules).map((p) => p.title)

    expect(titles).toEqual(['Alpha', 'Aardvark', 'Beta'])
  })

  it('rejects two pages with the same path', () => {
    const modules = { a: pageModule('/x', 'One', 1), b: pageModule('/x', 'Two', 2) }

    expect(() => buildPageRegistry(modules)).toThrow(PageRegistryError)
  })

  it('rejects a relative path', () => {
    const modules = { a: pageModule('x', 'One', 1) }

    expect(() => buildPageRegistry(modules)).toThrow(/must start with/)
  })

  it('rejects a module without a page export', () => {
    const modules = { 'broken.tsx': {} as PageModule }

    expect(() => buildPageRegistry(modules)).toThrow(/does not export/)
  })
})

describe('findPage', () => {
  const registered = buildPageRegistry({
    dashboard: pageModule('/dashboard', 'Dashboard', 0),
    agents: pageModule('/agents', 'Agents', 1),
  })

  it('finds the page for its own path and nested paths', () => {
    expect(findPage(registered, '/agents')?.title).toBe('Agents')
    expect(findPage(registered, '/agents/review-bot')?.title).toBe('Agents')
  })

  it('returns undefined for an unknown path', () => {
    expect(findPage(registered, '/missing')).toBeUndefined()
  })

  it('gives a nested page priority over its parent, in either registry order', () => {
    const childFirst = buildPageRegistry({
      child: pageModule('/agents/review-bot', 'Review bot', 0),
      parent: pageModule('/agents', 'Agents', 1),
    })
    const parentFirst = buildPageRegistry({
      parent: pageModule('/agents', 'Agents', 0),
      child: pageModule('/agents/review-bot', 'Review bot', 1),
    })

    expect(findPage(childFirst, '/agents/review-bot')?.title).toBe('Review bot')
    expect(findPage(parentFirst, '/agents/review-bot')?.title).toBe('Review bot')
  })

  it('matches a root page only at the root', () => {
    const withRoot = buildPageRegistry({ root: pageModule('/', 'Home', 0) })

    expect(findPage(withRoot, '/')?.title).toBe('Home')
    expect(findPage(withRoot, '/anything')).toBeUndefined()
  })
})
