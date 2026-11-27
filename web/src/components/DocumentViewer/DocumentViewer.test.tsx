import { render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { DocumentEntry } from '@/lib/documents'
import { DocumentViewer } from './DocumentViewer'

const htmlDocument: DocumentEntry = {
  path: 'dandelion/visuals/2026-11-27/1611-first-release.html',
  title: 'First release',
  kind: 'html',
  size: 4096,
  modified_ms: 1_700_000_000_000,
}

const markdownDocument: DocumentEntry = {
  path: 'dandelion/visuals/2026-11-27/notes.md',
  title: 'Notes',
  kind: 'markdown',
  size: 512,
  modified_ms: 1_700_000_000_000,
}

const fetchMock = vi.fn<(input: string, init?: RequestInit) => Promise<Response>>()

beforeEach(() => {
  fetchMock.mockReset()
  vi.stubGlobal('fetch', fetchMock)
})

describe('DocumentViewer', () => {
  it('frames an HTML document in an opaque-origin sandbox', () => {
    render(<DocumentViewer collection="probe-visuals" document={htmlDocument} />)

    const frame = screen.getByTitle('First release')
    expect(frame.tagName).toBe('IFRAME')
    expect(frame).toHaveAttribute('sandbox', 'allow-scripts')
    expect(frame.getAttribute('sandbox')).not.toContain('allow-same-origin')
    expect(frame).toHaveAttribute('referrerpolicy', 'no-referrer')
    expect(frame).toHaveAttribute(
      'src',
      '/api/documents/probe-visuals/file?path=dandelion%2Fvisuals%2F2026-11-27%2F1611-first-release.html',
    )
  })

  it('renders Markdown as text, never as raw HTML', async () => {
    fetchMock.mockResolvedValue(
      new Response('# Title\n\n<b>bold</b>\n\n| a | b |\n| - | - |\n| 1 | 2 |', { status: 200 }),
    )

    render(<DocumentViewer collection="probe-visuals" document={markdownDocument} />)

    expect(await screen.findByRole('heading', { name: 'Title' })).toBeInTheDocument()
    expect(screen.getByRole('table')).toBeInTheDocument()
    // The raw HTML tag is escaped text, not a bold element.
    expect(screen.getByText('<b>bold</b>')).toBeInTheDocument()
    expect(document.querySelector('b')).toBeNull()
    expect(fetchMock).toHaveBeenCalledWith(
      '/api/documents/probe-visuals/file?path=dandelion%2Fvisuals%2F2026-11-27%2Fnotes.md',
      expect.objectContaining({ credentials: 'same-origin' }),
    )
  })

  it('draws only inline images, never a remote source', async () => {
    fetchMock.mockResolvedValue(
      new Response(
        '![remote](https://example.test/pixel.png)\n\n![inline](data:image/png;base64,AA==)',
        {
          status: 200,
        },
      ),
    )

    render(<DocumentViewer collection="probe-visuals" document={markdownDocument} />)

    expect(await screen.findByAltText('inline')).toHaveAttribute(
      'src',
      'data:image/png;base64,AA==',
    )
    expect(screen.queryByAltText('remote')).toBeNull()
    expect(screen.getByText('remote')).toBeInTheDocument()
  })

  it('shows a failed read instead of the document', async () => {
    fetchMock.mockResolvedValue(new Response('', { status: 404 }))

    render(<DocumentViewer collection="probe-visuals" document={markdownDocument} />)

    expect(await screen.findByRole('alert')).toHaveTextContent('The server answered 404')
  })
})
