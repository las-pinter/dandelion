import { createContext, memo, useContext, useLayoutEffect, useMemo, useRef, useState } from 'react'
import ReactMarkdown, { type Components } from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { Button } from '@/components/ui/button'
import { useTranscript } from '@/hooks/use-transcript'
import {
  safeImageSource,
  subagentFor,
  type SubagentInfo,
  type TranscriptBlock,
  type TranscriptRecord,
} from '@/lib/transcript'
import { cn } from '@/lib/utils'

export interface TranscriptPanelProps {
  /** Claude Code session whose transcript is read. */
  sessionId: string | null
  /** A subagent file id; the nested panels pass one. */
  agent?: string | null
  /** Height and width classes for the panel. */
  className?: string
  /** The region's accessible name. */
  label?: string
}

/**
 * The conversation beside the raw pane.
 *
 * Read-only: the transcript comes from Claude Code's own file, unknown record
 * types never reach here, and the raw terminal stays the ground truth.
 */
export function TranscriptPanel({ sessionId, agent, className, label }: TranscriptPanelProps) {
  const state = useTranscript(sessionId, agent)

  if (!sessionId) {
    return (
      <p className="rounded-lg border border-dashed p-3 text-sm text-muted-foreground">
        No Claude Code session is behind this agent.
      </p>
    )
  }

  return (
    <TranscriptView
      className={className}
      label={label}
      sessionId={sessionId}
      records={state.records}
      subagents={state.subagents}
      hasEarlier={state.hasEarlier}
      loading={state.loading}
      error={state.error}
      onLoadEarlier={state.loadEarlier}
    />
  )
}

/** The conversation itself, without the socket; stories and tests render it. */
export interface TranscriptViewProps {
  /** Claude Code session whose subagents are addressable. */
  sessionId: string
  /** Records oldest first. */
  records: TranscriptRecord[]
  /** Subagent runs of the main session. */
  subagents: SubagentInfo[]
  /** Whether a page exists before the oldest record. */
  hasEarlier: boolean
  /** Whether the first page is still loading. */
  loading: boolean
  /** The newest socket or protocol problem. */
  error: string | null
  /** Asks for the page before the oldest record. */
  onLoadEarlier: () => void
  /** Height and width classes for the panel. */
  className?: string
  /** The region's accessible name. */
  label?: string
}

/** The conversation pane shared by the socket and the stories. */
export function TranscriptView({
  sessionId,
  records,
  subagents,
  hasEarlier,
  loading,
  error,
  onLoadEarlier,
  className,
  label = 'Transcript',
}: TranscriptViewProps) {
  // A tool result shows under its call; one whose call is on another page
  // stays a row of its own. Both lookups are derived once per page change.
  const { callIds, results } = useMemo(() => derive(records), [records])
  const scroller = useRef<HTMLDivElement>(null)
  const stick = useRef(true)
  const shown = useRef({ count: 0, first: null as number | null, height: 0 })

  // Follow the newest record, never yank a reader who scrolled back, and keep
  // the view in place when an earlier page is prepended.
  useLayoutEffect(() => {
    const element = scroller.current
    if (!element) return
    const previous = shown.current
    const first = records[0]?.offset ?? null
    if (previous.count === 0) {
      element.scrollTop = element.scrollHeight
    } else if (first === previous.first) {
      if (stick.current) element.scrollTop = element.scrollHeight
    } else if (records.length > previous.count) {
      element.scrollTop += element.scrollHeight - previous.height
    } else {
      element.scrollTop = element.scrollHeight
    }
    shown.current = { count: records.length, first, height: element.scrollHeight }
  }, [records])

  return (
    <div className={cn('flex h-96 flex-col gap-2', className)}>
      <div
        ref={scroller}
        className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto rounded-lg border bg-background p-3"
        role="log"
        aria-label={label}
        onScroll={(event) => {
          const element = event.currentTarget
          stick.current = element.scrollHeight - element.scrollTop - element.clientHeight < 48
        }}
      >
        <PinToBottom.Provider
          value={() => {
            const element = scroller.current
            if (element && stick.current) element.scrollTop = element.scrollHeight
          }}
        >
          {hasEarlier && (
            <div className="text-center">
              <Button variant="outline" size="xs" onClick={onLoadEarlier}>
                Load earlier
              </Button>
            </div>
          )}
          {records.map((record, index) => (
            <RecordView
              key={record.offset ?? index}
              record={record}
              results={results}
              callIds={callIds}
              subagents={subagents}
              sessionId={sessionId}
            />
          ))}
          {loading && <p className="text-sm text-muted-foreground">Reading the transcript…</p>}
          {!loading && records.length === 0 && !error && (
            <p className="text-sm text-muted-foreground">No messages yet.</p>
          )}
        </PinToBottom.Provider>
      </div>
      {error && (
        <p role="alert" className="text-xs text-destructive">
          {error}
        </p>
      )}
    </div>
  )
}

const ROLE_CLASS: Record<string, string> = {
  user: 'border-l-4 border-l-status-waiting bg-muted/40',
  assistant: 'border-l-4 border-l-primary',
  system: 'border-l-4 border-l-muted-foreground',
}

/** Whether a block has its own row: a tool result waits inside its call. */
function isShown(block: TranscriptBlock, callIds: Set<string>): boolean {
  return !(block.kind === 'tool_result' && block.toolUseId && callIds.has(block.toolUseId))
}

/** The call ids and the tool results keyed by the call they answer. */
function derive(records: TranscriptRecord[]) {
  const callIds = new Set<string>()
  const results = new Map<string, Extract<TranscriptBlock, { kind: 'tool_result' }>>()
  for (const record of records) {
    for (const block of record.blocks) {
      if (block.kind === 'tool_use') callIds.add(block.id)
      if (block.kind === 'tool_result' && block.toolUseId) {
        results.set(block.toolUseId, block)
      }
    }
  }
  return { callIds, results }
}

/** Re-pins the list to the newest record after content above it grows. */
const PinToBottom = createContext<() => void>(() => undefined)

function RecordView({
  record,
  results,
  callIds,
  subagents,
  sessionId,
}: {
  record: TranscriptRecord
  results: Map<string, Extract<TranscriptBlock, { kind: 'tool_result' }>>
  callIds: Set<string>
  subagents: SubagentInfo[]
  sessionId: string
}) {
  // A tool result with its call on the page shows inside the call, not here.
  const visible = record.blocks.filter((block) => isShown(block, callIds))
  if (visible.length === 0) return null
  return (
    <article
      className={cn(
        'min-w-0 rounded-md border py-2 pr-2 pl-3 [contain-intrinsic-size:auto_120px] [content-visibility:auto]',
        ROLE_CLASS[record.role],
      )}
    >
      <p className="mb-1 text-[10px] font-medium uppercase tracking-wide text-muted-foreground">
        {record.role}
        {record.at ? ` · ${formatTime(record.at)}` : ''}
      </p>
      <div className="flex min-w-0 flex-col gap-2">
        {visible.map((block, index) => (
          <BlockView
            key={index}
            block={block}
            results={results}
            callIds={callIds}
            subagents={subagents}
            sessionId={sessionId}
          />
        ))}
      </div>
    </article>
  )
}

function BlockView({
  block,
  results,
  callIds,
  subagents,
  sessionId,
}: {
  block: TranscriptBlock
  results: Map<string, Extract<TranscriptBlock, { kind: 'tool_result' }>>
  callIds: Set<string>
  subagents: SubagentInfo[]
  sessionId: string
}) {
  switch (block.kind) {
    case 'text':
      return <Markdown text={block.text} />
    case 'thinking':
      return (
        <details className="rounded border border-dashed p-2 text-xs">
          <summary className="cursor-pointer text-muted-foreground">Thinking</summary>
          <pre className="mt-2 max-h-64 overflow-auto whitespace-pre-wrap font-sans">
            {block.text}
          </pre>
        </details>
      )
    case 'tool_use': {
      const subagent = subagentFor(subagents, block.id)
      const result = results.get(block.id)
      return (
        <details className="rounded border p-2 text-xs">
          <summary className="cursor-pointer">
            <span className="font-mono font-medium">{block.name}</span>{' '}
            <span className="break-words text-muted-foreground">{block.summary}</span>
          </summary>
          <pre className="mt-2 max-h-64 overflow-auto whitespace-pre-wrap font-mono">
            {block.input}
          </pre>
          {result && (
            <div className="mt-2 border-t pt-2">
              <p className={cn('mb-1', result.isError && 'text-destructive')}>
                {result.isError ? 'Error' : 'Result'}
              </p>
              <pre className="max-h-64 overflow-auto whitespace-pre-wrap font-mono">
                {result.text}
              </pre>
            </div>
          )}
          {subagent?.agent && (
            <SubagentRun
              agent={subagent.agent}
              description={subagent.description ?? null}
              sessionId={sessionId}
            />
          )}
        </details>
      )
    }
    case 'tool_result':
      if (!isShown(block, callIds)) return null
      return (
        <details
          className={cn('rounded border p-2 text-xs', block.isError && 'border-destructive/50')}
        >
          <summary className="cursor-pointer text-muted-foreground">
            {block.isError ? 'Error' : 'Result'}
          </summary>
          <pre className="mt-2 max-h-64 overflow-auto whitespace-pre-wrap font-mono">
            {block.text}
          </pre>
        </details>
      )
    case 'divider':
      return (
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          <hr className="flex-1" />
          {block.label}
          <hr className="flex-1" />
        </div>
      )
  }
}

function SubagentRun({
  sessionId,
  agent,
  description,
}: {
  sessionId: string
  agent: string
  description: string | null
}) {
  const [open, setOpen] = useState(false)
  return (
    <details
      className="mt-2 rounded border border-dashed p-2"
      onToggle={(event) => setOpen(event.currentTarget.open)}
    >
      <summary className="cursor-pointer text-muted-foreground">
        Subagent{description ? `: ${description}` : ''}
      </summary>
      {open && (
        <TranscriptPanel
          className="mt-2"
          sessionId={sessionId}
          agent={agent}
          label={description ? `Subagent transcript: ${description}` : 'Subagent transcript'}
        />
      )}
    </details>
  )
}

/** Rendered markdown; raw HTML never reaches the DOM. */
const MARKDOWN_COMPONENTS: Components = {
  a: ({ href, children }) => (
    <a className="underline" href={href} target="_blank" rel="noopener noreferrer">
      {children}
    </a>
  ),
  img: TranscriptImage,
}

/** An https image that re-pins the list once it grows into place. */
function TranscriptImage({ src, alt }: { src?: string | Blob; alt?: string }) {
  const pin = useContext(PinToBottom)
  const safe = typeof src === 'string' ? safeImageSource(src) : null
  if (!safe) {
    return <span className="text-muted-foreground">[{alt ?? 'image'}]</span>
  }
  return (
    <img
      className="h-auto max-h-80 max-w-full rounded object-contain"
      src={safe}
      alt={alt ?? ''}
      loading="lazy"
      decoding="async"
      onLoad={pin}
    />
  )
}

const Markdown = memo(function Markdown({ text }: { text: string }) {
  return (
    <div className="max-w-none break-words text-sm leading-relaxed [&_a]:underline [&_code]:rounded [&_code]:bg-muted [&_code]:px-1 [&_code]:py-0.5 [&_code]:text-xs [&_h1]:mb-2 [&_h1]:text-base [&_h1]:font-semibold [&_h2]:mb-2 [&_h2]:text-sm [&_h2]:font-semibold [&_ol]:mb-2 [&_ol]:list-decimal [&_ol]:pl-5 [&_p]:mb-2 [&_pre]:mb-2 [&_pre]:overflow-x-auto [&_pre]:rounded [&_pre]:bg-muted [&_pre]:p-2 [&_ul]:mb-2 [&_ul]:list-disc [&_ul]:pl-5">
      <ReactMarkdown remarkPlugins={[remarkGfm]} components={MARKDOWN_COMPONENTS}>
        {text}
      </ReactMarkdown>
    </div>
  )
})

/** Short local time from an ISO timestamp, or the raw value when unparsable. */
function formatTime(at: string): string {
  const date = new Date(at)
  if (Number.isNaN(date.getTime())) return at
  return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
}
