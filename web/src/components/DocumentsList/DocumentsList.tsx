import type { DocumentSelection, DocumentsSnapshot } from '@/lib/documents'
import { documentIcon, formatModified, formatSize, groupLabel } from '@/lib/documents'
import { cn } from '@/lib/utils'

export interface DocumentsListProps {
  /** The collections, groups, and manifest warnings to show. */
  snapshot: DocumentsSnapshot
  /** The document open in the viewer. */
  selected: DocumentSelection | null
  /** Called with the document the owner picked. */
  onSelect: (selection: DocumentSelection) => void
  /** Height and width classes for the scrolling list. */
  className?: string
}

/**
 * The master side of the Documents page: collections, their groups, and the
 * manifest files that were skipped, each with the reason.
 */
export function DocumentsList({ snapshot, selected, onSelect, className }: DocumentsListProps) {
  const { collections, warnings } = snapshot

  if (collections.length === 0 && warnings.length === 0) {
    return (
      <p className="rounded-lg border border-dashed p-3 text-sm text-muted-foreground">
        No document collections yet. A plugin adds one by writing a manifest to the Dandelion plugin
        folder.
      </p>
    )
  }

  return (
    <div className={cn('flex min-w-0 flex-col gap-4', className)}>
      {warnings.length > 0 && (
        <section aria-label="Skipped manifests" className="rounded-lg border border-dashed p-3">
          <h3 className="text-xs font-semibold">Skipped manifests</h3>
          <ul className="mt-1 flex flex-col gap-1 text-xs text-muted-foreground">
            {warnings.map((warning) => (
              <li key={warning.file}>
                <span className="font-mono text-foreground">{warning.file}</span> —{' '}
                {warning.problem}
              </li>
            ))}
          </ul>
        </section>
      )}

      {collections.map((collection) => {
        const Icon = documentIcon(collection.icon)
        return (
          <section key={collection.id} aria-label={collection.name} className="min-w-0">
            <header className="flex items-center gap-2">
              <Icon aria-hidden className="size-4 text-muted-foreground" />
              <h3 className="text-sm font-semibold">{collection.name}</h3>
            </header>
            <p className="mt-0.5 truncate font-mono text-[11px] text-muted-foreground">
              {collection.root}
            </p>
            {collection.groups.length === 0 ? (
              <p className="mt-2 text-xs text-muted-foreground">No documents yet.</p>
            ) : (
              <div className="mt-2 flex flex-col gap-3">
                {collection.groups.map((group) => (
                  <div key={group.values.join('\u0000')}>
                    <h4 className="text-[11px] font-medium tracking-wide text-muted-foreground uppercase">
                      {groupLabel(group.values)}
                    </h4>
                    <ul className="mt-1 flex flex-col">
                      {group.documents.map((document) => {
                        const active =
                          selected?.collection === collection.id && selected.path === document.path
                        return (
                          <li key={document.path}>
                            <button
                              type="button"
                              onClick={() =>
                                onSelect({ collection: collection.id, path: document.path })
                              }
                              aria-current={active ? 'true' : undefined}
                              className={cn(
                                'flex w-full flex-col rounded-md px-2 py-1.5 text-left',
                                'hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none',
                                active && 'bg-muted',
                              )}
                            >
                              <span className="truncate text-sm">{document.title}</span>
                              <span className="truncate text-[11px] text-muted-foreground">
                                {document.kind === 'html' ? 'HTML' : 'Markdown'} ·{' '}
                                {formatSize(document.size)} · {formatModified(document.modified_ms)}
                              </span>
                            </button>
                          </li>
                        )
                      })}
                    </ul>
                  </div>
                ))}
              </div>
            )}
          </section>
        )
      })}
    </div>
  )
}
