import { describe, expect, it } from 'vitest'
import {
  documentIcon,
  fileUrl,
  findCollection,
  findDocument,
  formatSize,
  groupLabel,
  newestDocument,
  type DocumentsSnapshot,
} from './documents'

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
        {
          values: ['dandelion', '2026-11-26'],
          documents: [
            {
              path: 'dandelion/visuals/2026-11-26/0900-old.md',
              title: 'Old',
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

describe('document helpers', () => {
  it('encodes the collection and path into the file route', () => {
    expect(fileUrl('probe-visuals', 'a/b c.html')).toBe(
      '/api/documents/probe-visuals/file?path=a%2Fb%20c.html',
    )
  })

  it('names ungrouped documents and joins capture values', () => {
    expect(groupLabel([])).toBe('All documents')
    expect(groupLabel(['dandelion', '2026-11-27'])).toBe('dandelion · 2026-11-27')
  })

  it('formats sizes in bytes, kilobytes, and megabytes', () => {
    expect(formatSize(512)).toBe('512 B')
    expect(formatSize(2048)).toBe('2.0 kB')
    expect(formatSize(3 * 1024 * 1024)).toBe('3.0 MB')
  })

  it('maps the manifest icon vocabulary with a default', () => {
    expect(documentIcon('chart')).not.toBe(documentIcon('document'))
    expect(documentIcon('sparkles')).toBe(documentIcon('document'))
  })

  it('finds a document and its collection only while the snapshot holds them', () => {
    const selection = {
      collection: 'probe-visuals',
      path: 'dandelion/visuals/2026-11-26/0900-old.md',
    }

    expect(findDocument(snapshot, selection)?.title).toBe('Old')
    expect(findCollection(snapshot, selection)?.name).toBe('Probe visuals')
    expect(findDocument(snapshot, { collection: 'gone', path: 'x.md' })).toBeNull()
    expect(findDocument(snapshot, null)).toBeNull()
  })

  it('opens the newest document when nothing is chosen', () => {
    expect(newestDocument(snapshot)).toEqual({
      collection: 'probe-visuals',
      path: 'dandelion/visuals/2026-11-27/1611-first-release.html',
    })
    expect(newestDocument({ collections: [], warnings: [] })).toBeNull()
  })
})
