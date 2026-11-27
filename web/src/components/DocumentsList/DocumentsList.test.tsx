import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import type { DocumentSelection, DocumentsSnapshot } from '@/lib/documents'
import { DocumentsList } from './DocumentsList'

const snapshot: DocumentsSnapshot = {
  collections: [
    {
      id: 'probe-visuals',
      name: 'Probe visuals',
      icon: 'chart',
      root: '~/notes',
      groups: [
        {
          values: ['dandelion', '2026-11-27'],
          documents: [
            {
              path: 'dandelion/visuals/2026-11-27/1611-first-release.html',
              title: 'First release',
              kind: 'html',
              size: 4096,
              modified_ms: 1_700_000_000_000,
            },
          ],
        },
      ],
    },
  ],
  warnings: [],
}

function setup(overrides: Partial<Parameters<typeof DocumentsList>[0]> = {}) {
  const onSelect = vi.fn<(selection: DocumentSelection) => void>()
  render(<DocumentsList snapshot={snapshot} selected={null} onSelect={onSelect} {...overrides} />)
  return { onSelect }
}

describe('DocumentsList', () => {
  it('groups documents under their capture values', () => {
    setup()

    expect(screen.getByRole('heading', { name: 'Probe visuals' })).toBeInTheDocument()
    expect(screen.getByText('~/notes')).toBeInTheDocument()
    expect(screen.getByText('dandelion · 2026-11-27')).toBeInTheDocument()
    expect(screen.getByText('First release')).toBeInTheDocument()
  })

  it('reports a document with its kind, size, and selection', async () => {
    const user = userEvent.setup()
    const { onSelect } = setup({
      selected: {
        collection: 'probe-visuals',
        path: 'dandelion/visuals/2026-11-27/1611-first-release.html',
      },
    })

    const button = screen.getByRole('button', { name: /First release/ })
    expect(button).toHaveAttribute('aria-current', 'true')
    expect(button).toHaveTextContent('HTML')
    expect(button).toHaveTextContent('4.0 kB')

    await user.click(button)

    expect(onSelect).toHaveBeenCalledWith({
      collection: 'probe-visuals',
      path: 'dandelion/visuals/2026-11-27/1611-first-release.html',
    })
  })

  it('shows each skipped manifest with its reason', () => {
    setup({
      snapshot: { ...snapshot, warnings: [{ file: 'broken.json', problem: 'not valid JSON' }] },
    })

    expect(screen.getByText('Skipped manifests')).toBeInTheDocument()
    expect(screen.getByText('broken.json')).toBeInTheDocument()
    expect(screen.getByText(/not valid JSON/)).toBeInTheDocument()
  })

  it('explains an empty page and an empty collection', () => {
    setup({ snapshot: { collections: [], warnings: [] } })

    expect(screen.getByText(/No document collections yet/)).toBeInTheDocument()
  })

  it('says a collection has no documents yet', () => {
    setup({ snapshot: { ...snapshot, collections: [{ ...snapshot.collections[0]!, groups: [] }] } })

    expect(screen.getByText('No documents yet.')).toBeInTheDocument()
  })
})
