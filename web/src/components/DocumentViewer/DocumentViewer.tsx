import { useEffect, useState } from 'react'
import ReactMarkdown, { defaultUrlTransform, type Components } from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { fileUrl, type DocumentEntry } from '@/lib/documents'
import { cn } from '@/lib/utils'

export interface DocumentViewerProps {
  /** The collection the document belongs to. */
  collection: string
  /** The document to show. */
  document: DocumentEntry
  /** Height and width classes for the viewer. */
  className?: string
}

const MARKDOWN_COMPONENTS: Components = {
  h1: ({ children }) => (
    <h1 className="mt-4 mb-2 font-heading text-xl font-semibold">{children}</h1>
  ),
  h2: ({ children }) => (
    <h2 className="mt-4 mb-2 font-heading text-lg font-semibold">{children}</h2>
  ),
  h3: ({ children }) => (
    <h3 className="mt-3 mb-1 font-heading text-base font-semibold">{children}</h3>
  ),
  p: ({ children }) => <p className="my-2 leading-relaxed">{children}</p>,
  ul: ({ children }) => <ul className="my-2 ml-5 list-disc">{children}</ul>,
  ol: ({ children }) => <ol className="my-2 ml-5 list-decimal">{children}</ol>,
  code: ({ children, className }) =>
    className ? (
      <code className={cn(className, 'font-mono text-xs')}>{children}</code>
    ) : (
      <code className="rounded bg-muted px-1 py-0.5 font-mono text-xs">{children}</code>
    ),
  pre: ({ children }) => (
    <pre className="my-2 overflow-x-auto rounded-lg bg-muted p-3 text-xs">{children}</pre>
  ),
  table: ({ children }) => (
    <div className="my-2 overflow-x-auto">
      <table className="w-full border-collapse text-sm">{children}</table>
    </div>
  ),
  th: ({ children }) => <th className="border-b px-2 py-1 text-left font-semibold">{children}</th>,
  td: ({ children }) => <td className="border-b px-2 py-1">{children}</td>,
  a: ({ children, href }) => (
    <a
      href={href}
      target="_blank"
      rel="noopener noreferrer"
      className="text-primary underline underline-offset-2"
    >
      {children}
    </a>
  ),
  // Documents are untrusted: only an inline image is drawn, so a Markdown
  // file cannot make the app origin fetch a remote beacon.
  img: ({ src, alt }) =>
    typeof src === 'string' && src.startsWith('data:image/') ? (
      <img src={src} alt={alt ?? ''} className="my-2 max-w-full rounded" />
    ) : (
      <span className="text-muted-foreground italic">{alt || 'image'}</span>
    ),
}

/** Inline images pass the URL guard; everything else follows the default. */
function documentUrlTransform(url: string): string {
  return url.startsWith('data:image/') ? url : defaultUrlTransform(url)
}

type TextState =
  { status: 'loading' } | { status: 'error'; message: string } | { status: 'ready'; text: string }

/**
 * Shows one document.
 *
 * HTML comes in through an iframe with `sandbox="allow-scripts"` and no
 * `allow-same-origin`, so the document runs in an opaque origin: its scripts
 * cannot call the API or read the session cookie. Markdown is fetched as
 * text and rendered by the app with raw HTML disabled.
 */
export function DocumentViewer({ collection, document, className }: DocumentViewerProps) {
  if (document.kind === 'html') {
    return (
      <iframe
        key={`${collection}:${document.path}`}
        title={document.title}
        src={fileUrl(collection, document.path)}
        sandbox="allow-scripts"
        referrerPolicy="no-referrer"
        className={cn('w-full rounded-lg border bg-white', className)}
      />
    )
  }
  return <MarkdownDocument collection={collection} document={document} className={className} />
}

function MarkdownDocument({ collection, document, className }: DocumentViewerProps) {
  const url = fileUrl(collection, document.path)
  // The text is stored with the URL it belongs to, so a changed document is
  // "loading" without a synchronous state write inside the effect.
  const [result, setResult] = useState<{ url: string; state: TextState } | null>(null)

  useEffect(() => {
    const controller = new AbortController()
    void (async () => {
      try {
        const response = await fetch(url, {
          credentials: 'same-origin',
          signal: controller.signal,
        })
        if (!response.ok) {
          throw new Error(`The server answered ${String(response.status)}`)
        }
        const text = await response.text()
        setResult({ url, state: { status: 'ready', text } })
      } catch (cause: unknown) {
        if (controller.signal.aborted) return
        setResult({
          url,
          state: {
            status: 'error',
            message: cause instanceof Error ? cause.message : 'cannot read the document',
          },
        })
      }
    })()
    return () => {
      controller.abort()
    }
  }, [url])

  const state: TextState = result?.url === url ? result.state : { status: 'loading' }

  if (state.status === 'loading') {
    return <p className={cn('text-sm text-muted-foreground', className)}>Reading the document…</p>
  }
  if (state.status === 'error') {
    return (
      <p role="alert" className={cn('text-sm text-destructive', className)}>
        {state.message}
      </p>
    )
  }
  return (
    <div
      className={cn('overflow-auto rounded-lg border p-4 text-sm text-card-foreground', className)}
    >
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        components={MARKDOWN_COMPONENTS}
        urlTransform={documentUrlTransform}
      >
        {state.text}
      </ReactMarkdown>
    </div>
  )
}
