import { useEffect, useId, useRef, useState } from 'react'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Panel } from '@/components/Panel/Panel'
import { TranscriptPanel } from '@/components/TranscriptPanel/TranscriptPanel'
import { StatusPill } from '@/components/StatusPill/StatusPill'
import { XtermPane } from '@/components/XtermPane/XtermPane'
import { cn } from '@/lib/utils'
import {
  agentLabel,
  agentSocketUrl,
  blockedLabel,
  claudeSummary,
  uiStatus,
  type AgentAction,
  type AgentInfo,
  type ClaudeAgent,
} from '@/lib/agents'

export interface AgentDetailProps {
  agent: AgentInfo
  /** The Claude Code session behind this terminal, when one was found. */
  claude?: ClaudeAgent | null
  /** The action in flight, as `terminal:kind`. */
  busy?: string | null
  onAct: (action: AgentAction) => void
}

/** One agent: its live pane, the prompt box, and the control buttons. */
export function AgentDetail({ agent, claude, busy, onAct }: AgentDetailProps) {
  const [prompt, setPrompt] = useState('')
  const [confirmKill, setConfirmKill] = useState(false)
  const [paneError, setPaneError] = useState<string | null>(null)
  const [view, setView] = useState<'terminal' | 'transcript'>('terminal')
  const [expanded, setExpanded] = useState(false)
  const overlay = useRef<HTMLDivElement>(null)
  const promptId = useId()
  const status = uiStatus(agent)
  const blocked = blockedLabel(agent.agent_state)
  const session = claude ? claudeSummary(claude) : null
  const inFlight = (kind: string) => busy === `${agent.id}:${kind}`

  // The expanded view is modal: Escape closes it and Tab stays inside it.
  useEffect(() => {
    if (!expanded) return
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        setExpanded(false)
        return
      }
      if (event.key !== 'Tab') return
      const element = overlay.current
      if (!element) return
      const focusable = element.querySelectorAll<HTMLElement>(
        'button, [href], input, textarea, select, [tabindex]:not([tabindex="-1"])',
      )
      const first = focusable[0]
      const last = focusable[focusable.length - 1]
      if (!first || !last) return
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault()
        last.focus()
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault()
        first.focus()
      }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [expanded])

  const showTranscript = view === 'transcript' && claude !== null

  const send = () => {
    if (prompt.trim() === '') return
    onAct({ kind: 'prompt', text: prompt })
    setPrompt('')
  }

  return (
    <Panel
      title={agentLabel(agent)}
      description={`${agent.agent_kind ?? 'agent'} · ${agent.cwd ?? 'unknown directory'}`}
      status={status}
      actions={<StatusPill status={status} />}
    >
      <p className="mb-3 text-xs text-muted-foreground" aria-live="polite">
        {blocked ?? 'Nothing is waiting on you.'}
      </p>
      {session && (
        <dl className="mb-3 grid grid-cols-2 gap-x-4 gap-y-1 rounded-lg border border-dashed p-3 text-xs sm:grid-cols-4">
          <div className="min-w-0">
            <dt className="text-muted-foreground">Claude session</dt>
            <dd className="truncate font-mono" title={session.sessionId}>
              {session.sessionId.slice(0, 8)}
            </dd>
          </div>
          <div className="min-w-0">
            <dt className="text-muted-foreground">Model</dt>
            <dd className="truncate">{session.model}</dd>
          </div>
          <div>
            <dt className="text-muted-foreground">Tokens</dt>
            <dd className="tabular-nums">{session.tokens}</dd>
          </div>
          <div>
            <dt className="text-muted-foreground">Est. cost</dt>
            <dd className="tabular-nums">{session.cost}</dd>
          </div>
          {session.waiting && (
            <div className="col-span-2 sm:col-span-4">
              <dt className="text-muted-foreground">Waiting for</dt>
              <dd>{session.waiting}</dd>
            </div>
          )}
        </dl>
      )}
      <div className="mb-3 flex flex-wrap gap-1">
        <Button
          size="xs"
          disabled={inFlight('approve') || !blocked}
          title={blocked ? undefined : 'no prompt is waiting'}
          onClick={() => onAct({ kind: 'approve' })}
        >
          Approve
        </Button>
        <Button
          size="xs"
          variant="outline"
          disabled={inFlight('deny') || !blocked}
          onClick={() => onAct({ kind: 'deny' })}
        >
          Deny
        </Button>
        <Button
          size="xs"
          variant="outline"
          disabled={inFlight('interrupt')}
          onClick={() => onAct({ kind: 'interrupt' })}
        >
          Interrupt
        </Button>
        <Button
          size="xs"
          variant="ghost"
          disabled={inFlight('kill')}
          onClick={() => setConfirmKill(true)}
        >
          Kill
        </Button>
      </div>
      <form
        className="mb-3 flex gap-2"
        onSubmit={(event) => {
          event.preventDefault()
          send()
        }}
      >
        <label htmlFor={promptId} className="sr-only">
          Prompt
        </label>
        <Input
          id={promptId}
          value={prompt}
          onChange={(event) => setPrompt(event.target.value)}
          placeholder="Send a prompt to this agent"
        />
        <Button type="submit" size="sm" disabled={inFlight('prompt') || prompt.trim() === ''}>
          Send
        </Button>
      </form>
      <div
        ref={overlay}
        role={expanded ? 'dialog' : undefined}
        aria-modal={expanded || undefined}
        aria-label={expanded ? 'Expanded agent view' : undefined}
        className={
          expanded
            ? 'fixed inset-2 z-50 flex flex-col gap-2 overscroll-contain rounded-xl border bg-popover p-3 text-popover-foreground shadow-2xl'
            : 'flex flex-col gap-2'
        }
      >
        <div role="group" aria-label="Agent view" className="flex items-center gap-1">
          {claude && (
            <>
              <Button
                size="xs"
                aria-pressed={view === 'terminal'}
                variant={view === 'terminal' ? 'secondary' : 'ghost'}
                onClick={() => {
                  setView('terminal')
                  setPaneError(null)
                }}
              >
                Terminal
              </Button>
              <Button
                size="xs"
                aria-pressed={view === 'transcript'}
                variant={view === 'transcript' ? 'secondary' : 'ghost'}
                onClick={() => setView('transcript')}
              >
                Transcript
              </Button>
            </>
          )}
          <span className="flex-1" />
          <Button size="xs" variant="ghost" onClick={() => setExpanded((open) => !open)}>
            {expanded ? 'Exit full screen' : 'Full screen'}
          </Button>
        </div>
        {showTranscript && claude ? (
          <TranscriptPanel
            sessionId={claude.sessionId}
            className={expanded ? 'min-h-0 flex-1' : 'h-[60vh] sm:h-96'}
          />
        ) : (
          <div
            className={cn(
              'overflow-hidden rounded-lg border bg-background',
              expanded ? 'min-h-0 flex-1' : 'h-[60vh] sm:h-96',
            )}
          >
            <XtermPane
              url={agentSocketUrl(agent.id)}
              readOnly
              onClosedWithoutOpen={(reason) =>
                setPaneError(reason ?? 'the agent pane could not attach')
              }
            />
          </div>
        )}
        {!showTranscript && paneError && (
          <p role="alert" className="text-xs text-destructive">
            {paneError}
          </p>
        )}
      </div>

      <Dialog open={confirmKill} onOpenChange={setConfirmKill}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Kill {agentLabel(agent)}?</DialogTitle>
            <DialogDescription>
              This ends the agent's terminal and the program running in it.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter showCloseButton>
            <Button
              variant="destructive"
              onClick={() => {
                onAct({ kind: 'kill' })
                setConfirmKill(false)
              }}
            >
              Kill
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </Panel>
  )
}
