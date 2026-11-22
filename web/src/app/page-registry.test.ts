import { LayoutDashboard } from 'lucide-react'
import { buildPageRegistry, PageRegistryError, type PageModule } from './page-registry'

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
