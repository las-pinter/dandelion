import { BookOpen } from 'lucide-react'
import { useSearchParams } from 'react-router'
import type { PageDefinition } from '@/app/page-registry'
import { DocumentsList } from '@/components/DocumentsList/DocumentsList'
import { DocumentViewer } from '@/components/DocumentViewer/DocumentViewer'
import { Panel } from '@/components/Panel/Panel'
import { Button } from '@/components/ui/button'
import { useDocuments } from '@/hooks/use-documents'
import {
  findCollection,
  findDocument,
  formatModified,
  newestDocument,
  type DocumentSelection,
} from '@/lib/documents'

const VIEWER_HEIGHT = 'h-[70svh] min-h-[24rem]'

function selectionFrom(params: URLSearchParams): DocumentSelection | null {
  const collection = params.get('doc')
  const path = params.get('path')
  return collection && path ? { collection, path } : null
}

export function DocumentsPage() {
  const { snapshot, loading, error, refresh } = useDocuments()
  const [params, setParams] = useSearchParams()

  const requested = selectionFrom(params)
  const chosen = snapshot && findDocument(snapshot, requested) ? requested : null

  // A page opened without a valid selection shows the newest document, and
  // the URL takes the choice as soon as the owner picks one, so a document
  // can be bookmarked and survives a reload.
  const shown = chosen ?? (snapshot ? newestDocument(snapshot) : null)
  const shownDocument = snapshot ? findDocument(snapshot, shown) : null
  const shownCollection = snapshot ? findCollection(snapshot, shown) : null

  const select = (next: DocumentSelection) => {
    setParams({ doc: next.collection, path: next.path }, { replace: true })
  }

  return (
    <div className="flex flex-col gap-4">
      <Panel
        title="Documents"
        description="Collections declared by manifests in the Dandelion plugin folder. HTML documents render in a sandboxed frame; Markdown renders in the app."
        actions={
          <Button variant="outline" size="sm" onClick={refresh}>
            Refresh
          </Button>
        }
      >
        {error && (
          <p role="alert" className="mb-3 text-sm text-destructive">
            {error}
          </p>
        )}
        {loading && !snapshot ? (
          <p className="py-8 text-center text-sm text-muted-foreground">Reading documents…</p>
        ) : snapshot ? (
          <div className="grid gap-4 md:grid-cols-[minmax(15rem,20rem)_minmax(0,1fr)]">
            <DocumentsList
              snapshot={snapshot}
              selected={shown}
              onSelect={select}
              className="md:max-h-[70svh] md:overflow-y-auto"
            />
            {shownDocument && shownCollection ? (
              <div className="flex min-w-0 flex-col gap-2">
                <header className="min-w-0">
                  <h2 className="truncate font-heading text-base font-semibold">
                    {shownDocument.title}
                  </h2>
                  <p className="truncate text-xs text-muted-foreground">
                    {shownCollection.name} · {shownDocument.path} ·{' '}
                    {formatModified(shownDocument.modified_ms)}
                  </p>
                </header>
                <DocumentViewer
                  key={`${shownCollection.id}:${shownDocument.path}`}
                  collection={shownCollection.id}
                  document={shownDocument}
                  className={VIEWER_HEIGHT}
                />
              </div>
            ) : (
              <p className="rounded-lg border border-dashed p-3 text-sm text-muted-foreground">
                Pick a document to read it.
              </p>
            )}
          </div>
        ) : null}
      </Panel>
    </div>
  )
}

export const page: PageDefinition = {
  path: '/documents',
  title: 'Documents',
  icon: BookOpen,
  order: 6,
  Component: DocumentsPage,
}
