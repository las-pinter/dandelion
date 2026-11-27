import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter, Route, Routes, useLocation } from 'react-router'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { DocumentsState } from '@/hooks/use-documents'
import type { DocumentsSnapshot } from '@/lib/documents'
import { DocumentsPage } from './screen'

const { useDocuments } = vi.hoisted(() => ({ useDocuments: vi.fn<() => DocumentsState>() }))

vi.mock('@/hooks/use-documents', () => ({ useDocuments }))

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
            {
              path: 'dandelion/visuals/2026-11-27/0900-older.md',
              title: 'Older notes',
              kind: 'markdown',
              size: 512,
              modified_ms: 1_699_000_000_000,
            },
          ],
        },
      ],
    },
  ],
  warnings: [],
}

function LocationProbe() {
  const location = useLocation()
  return <p data-testid="location">{location.search}</p>
}

function setup(url = '/documents', state: Record<string, unknown> = {}) {
  useDocuments.mockReturnValue(snapshotState(state))
  render(
    <MemoryRouter initialEntries={[url]}>
      <Routes>
        <Route
          path="/documents"
          element={
            <>
              <DocumentsPage />
              <LocationProbe />
            </>
          }
        />
      </Routes>
    </MemoryRouter>,
  )
}

function snapshotState(overrides: Record<string, unknown>) {
  return {
    snapshot,
    loading: false,
    error: null,
    refresh: vi.fn<() => void>(),
    ...overrides,
  }
}

beforeEach(() => {
  useDocuments.mockReset()
})

describe('DocumentsPage', () => {
  it('opens the newest document when the URL names none', () => {
    setup()

    expect(screen.getByRole('heading', { name: 'First release' })).toBeInTheDocument()
    expect(screen.getByTitle('First release')).toHaveAttribute('sandbox', 'allow-scripts')
  })

  it('honours a document the URL names', () => {
    setup('/documents?doc=probe-visuals&path=dandelion%2Fvisuals%2F2026-11-27%2F0900-older.md')

    expect(screen.getByRole('heading', { name: 'Older notes' })).toBeInTheDocument()
  })

  it('falls back to the newest document when the URL names a missing one', () => {
    setup('/documents?doc=gone&path=missing.md')

    expect(screen.getByRole('heading', { name: 'First release' })).toBeInTheDocument()
  })

  it('puts the chosen document in the URL', async () => {
    const user = userEvent.setup()
    setup()

    await user.click(screen.getByRole('button', { name: /Older notes/ }))

    expect(screen.getByRole('heading', { name: 'Older notes' })).toBeInTheDocument()
    expect(screen.getByTestId('location')).toHaveTextContent(
      '?doc=probe-visuals&path=dandelion%2Fvisuals%2F2026-11-27%2F0900-older.md',
    )
  })

  it('reports a failed listing', () => {
    useDocuments.mockReturnValue(snapshotState({ snapshot: null, error: 'the listing failed' }))
    render(
      <MemoryRouter initialEntries={['/documents']}>
        <DocumentsPage />
      </MemoryRouter>,
    )

    expect(screen.getByRole('alert')).toHaveTextContent('the listing failed')
  })
})
