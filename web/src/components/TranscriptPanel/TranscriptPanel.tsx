import {
  createContext,
  memo,
  useContext,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from 'react'
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
  const groups = useMemo(() => groupRecords(records, callIds), [records, callIds])
  const scroller = useRef<HTMLDivElement>(null)
  const content = useRef<HTMLDivElement>(null)
  const stick = useRef(true)
  const gesture = useRef(0)
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

  const markGesture = () => {
    gesture.current = Date.now()
  }

  // A chat follows its newest line while the reader is at the bottom. Two
  // things move under that rule without a new record: rows whose real height
  // replaces the `content-visibility` estimate (and images, and markdown that
  // settles late), and the viewport itself when the live strip opens or
  // closes. The first re-pins; the second keeps the bottom anchored by the
  // height it lost, so the reader's place is not silently abandoned.
  useEffect(() => {
    const element = scroller.current
    const rows = content.current
    if (!element || !rows || typeof ResizeObserver === 'undefined') return
    let viewport: number | null = null
    const followRows = new ResizeObserver(() => {
      if (stick.current) element.scrollTop = element.scrollHeight
    })
    const followViewport = new ResizeObserver(() => {
      const next = element.clientHeight
      if (viewport !== null && stick.current) element.scrollTop += viewport - next
      viewport = next
    })
    followRows.observe(rows)
    followViewport.observe(element)
    return () => {
      followRows.disconnect()
      followViewport.disconnect()
    }
  }, [])

  return (
    <div className={cn('flex h-96 flex-col gap-2', className)}>
      <div
        ref={scroller}
        className="flex min-h-0 flex-1 flex-col overflow-y-auto px-1 py-2 [overflow-anchor:none]"
        role="log"
        aria-label={label}
        // Only someone's own scrolling leaves the bottom; a scroll event a
        // resize produces must not silently stop the follow.
        onWheel={markGesture}
        onTouchStart={markGesture}
        onTouchMove={markGesture}
        onPointerDown={markGesture}
        onScroll={(event) => {
          const element = event.currentTarget
          const nearBottom = element.scrollHeight - element.scrollTop - element.clientHeight < 48
          if (nearBottom) stick.current = true
          else if (Date.now() - gesture.current < 1_000) stick.current = false
        }}
      >
        <PinToBottom.Provider
          value={() => {
            const element = scroller.current
            if (element && stick.current) element.scrollTop = element.scrollHeight
          }}
        >
          <div ref={content} className="flex w-full max-w-[84ch] shrink-0 flex-col gap-6">
            {hasEarlier && (
              <div className="text-center">
                <Button variant="ghost" size="xs" onClick={onLoadEarlier}>
                  Load earlier
                </Button>
              </div>
            )}
            {groups.map((group) => (
              <div
                key={group.key}
                className={cn(
                  'border-l-2 border-l-border pl-3',
                  group.role === 'user' && 'rounded-r-lg bg-secondary/40 py-1.5 pr-2',
                )}
              >
                <p className="mb-2 flex items-baseline gap-2 text-[11px] text-muted-foreground">
                  <span className={cn('font-medium', ROLE_TEXT[group.role])}>{group.role}</span>
                  {group.at && (
                    <span className="font-mono tabular-nums">
                      {formatTime(group.at)}
                      {group.end && group.end !== group.at && `–${formatTime(group.end)}`}
                    </span>
                  )}
                </p>
                <div className="flex flex-col gap-3">
                  {group.records.map((record, index) => (
                    <RecordView
                      key={record.offset ?? `${group.key}-${index}`}
                      record={record}
                      results={results}
                      callIds={callIds}
                      subagents={subagents}
                      sessionId={sessionId}
                    />
                  ))}
                </div>
              </div>
            ))}
            {loading && <p className="text-sm text-muted-foreground">Reading the transcript…</p>}
            {!loading && records.length === 0 && !error && (
              <p className="text-sm text-muted-foreground">No messages yet.</p>
            )}
          </div>
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

const ROLE_TEXT: Record<string, string> = {
  user: 'text-status-waiting',
  assistant: 'text-primary',
  system: 'text-muted-foreground',
}

/** Consecutive records from one role read as one message and share a header. */
interface Group {
  key: number
  role: string
  /** The first record's time, and the last one's when it differs. */
  at?: string
  end?: string
  records: TranscriptRecord[]
}

function groupRecords(records: TranscriptRecord[], callIds: Set<string>): Group[] {
  const groups: Group[] = []
  for (const record of records) {
    // A record whose blocks are all hidden (a tool result inside its call)
    // would leave an empty bubble, so it never starts a group.
    if (!record.blocks.some((block) => isShown(block, callIds))) continue
    const last = groups[groups.length - 1]
    if (last && last.role === record.role) {
      last.records.push(record)
      if (record.at) last.end = record.at
    } else {
      groups.push({
        key: record.offset ?? groups.length,
        role: record.role,
        at: record.at ?? undefined,
        records: [record],
      })
    }
  }
  return groups
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
    <article className="min-w-0 [contain-intrinsic-size:auto_120px] [content-visibility:auto]">
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
        <details className="rounded-md border border-transparent px-2 py-1 text-xs transition-colors hover:border-border hover:bg-muted/30">
          <summary className="cursor-pointer text-muted-foreground">Thinking</summary>
          <pre className="mt-2 max-h-64 overflow-auto whitespace-pre-wrap font-sans text-foreground/80">
            {block.text}
          </pre>
        </details>
      )
    case 'tool_use': {
      const subagent = subagentFor(subagents, block.id)
      const result = results.get(block.id)
      return (
        <details className="rounded-md border border-transparent px-2 py-1 text-xs transition-colors hover:border-border hover:bg-muted/30">
          <summary className="flex cursor-pointer items-baseline gap-2">
            <span className="shrink-0 font-mono font-medium">{block.name}</span>
            <span className="min-w-0 truncate text-muted-foreground">{block.summary}</span>
          </summary>
          <pre className="mt-2 max-h-64 overflow-auto rounded-md bg-background/70 p-2 whitespace-pre-wrap font-mono">
            {block.input}
          </pre>
          {result && (
            <div className="mt-2 border-t pt-2">
              <p className={cn('mb-1 text-muted-foreground', result.isError && 'text-destructive')}>
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
          className={cn(
            'rounded-md border border-transparent px-2 py-1 text-xs',
            block.isError && 'border-destructive/40',
          )}
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
        <div className="flex items-center gap-3 text-[11px] text-muted-foreground">
          <span className="h-px flex-1 bg-border" />
          {block.label}
          <span className="h-px flex-1 bg-border" />
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
      referrerPolicy="no-referrer"
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
