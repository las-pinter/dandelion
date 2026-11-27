import {
  ChartNoAxesColumn,
  Code,
  FileText,
  Image,
  Presentation,
  type LucideIcon,
} from 'lucide-react'
import type { components } from '@/api/gen/schema'

export type DocumentsSnapshot = components['schemas']['DocumentsSnapshot']
export type DocumentCollection = components['schemas']['Collection']
export type DocumentGroup = components['schemas']['Group']
export type DocumentEntry = components['schemas']['DocumentEntry']
export type DocumentKind = components['schemas']['DocumentKind']
export type DocumentWarning = components['schemas']['Warning']

/** One chosen document: the collection id and the path inside it. */
export interface DocumentSelection {
  collection: string
  path: string
}

const ICONS: Record<string, LucideIcon> = {
  document: FileText,
  chart: ChartNoAxesColumn,
  slides: Presentation,
  image: Image,
  code: Code,
}

/** The icon a manifest asked for, or the document default. */
export function documentIcon(name: string): LucideIcon {
  return ICONS[name] ?? FileText
}

/** The document route with the path encoded as one query value. */
export function fileUrl(collection: string, path: string): string {
  return `/api/documents/${encodeURIComponent(collection)}/file?path=${encodeURIComponent(path)}`
}

/** The group's capture values as one line, or a name for an ungrouped list. */
export function groupLabel(values: string[]): string {
  return values.length === 0 ? 'All documents' : values.join(' · ')
}

/** A file size for the row: bytes below 1 kB, then one decimal. */
export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} kB`
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}

/** The modification time as a short local date. */
export function formatModified(ms: number): string {
  if (ms === 0) return 'unknown date'
  return new Date(ms).toLocaleString(undefined, {
    dateStyle: 'medium',
    timeStyle: 'short',
  })
}

/** The chosen document's entry, when the snapshot still holds it. */
export function findDocument(
  snapshot: DocumentsSnapshot,
  selection: DocumentSelection | null,
): DocumentEntry | null {
  if (!selection) return null
  const collection = snapshot.collections.find((entry) => entry.id === selection.collection)
  if (!collection) return null
  for (const group of collection.groups) {
    const found = group.documents.find((document) => document.path === selection.path)
    if (found) return found
  }
  return null
}

/** The collection behind a selection, when the snapshot still holds it. */
export function findCollection(
  snapshot: DocumentsSnapshot,
  selection: DocumentSelection | null,
): DocumentCollection | null {
  if (!selection) return null
  return snapshot.collections.find((entry) => entry.id === selection.collection) ?? null
}

/** The first document of the newest group, for a page that opens with none chosen. */
export function newestDocument(snapshot: DocumentsSnapshot): DocumentSelection | null {
  const collection = snapshot.collections[0]
  const document = collection?.groups[0]?.documents[0]
  if (!collection || !document) return null
  return { collection: collection.id, path: document.path }
}
