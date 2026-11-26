import { useEffect, useRef, useState } from 'react'
import { AgentChat } from '@/components/AgentChat/AgentChat'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Panel } from '@/components/Panel/Panel'
import {
  agentLabel,
  asksQuestion,
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

/** One agent: its status, the controls, and the chat surface. */
export function AgentDetail({ agent, claude, busy, onAct }: AgentDetailProps) {
  const [confirmKill, setConfirmKill] = useState(false)
  const [expanded, setExpanded] = useState(false)
  const overlay = useRef<HTMLDivElement>(null)
  const trigger = useRef<HTMLButtonElement>(null)
  const wasExpanded = useRef(false)
  const status = uiStatus(agent)
  const blocked = blockedLabel(agent.agent_state)
  const question = asksQuestion(agent.agent_state)
  const session = claude ? claudeSummary(claude) : null
  const inFlight = (kind: string) => busy === `${agent.id}:${kind}`

  // The expanded view is modal: Escape closes it and focus stays inside it.
  // The trigger lives in the panel header, behind the overlay, and the list of
  // focusable controls has disabled entries a Tab skips; so the key wrap is
  // backed by a focus guard that pulls focus back whenever it lands outside.
  useEffect(() => {
    if (!expanded) return
    const focusableIn = (element: HTMLElement) =>
      element.querySelectorAll<HTMLElement>(
        'button:not([disabled]), [href], input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])',
      )
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        setExpanded(false)
        return
      }
      if (event.key !== 'Tab') return
      const element = overlay.current
      if (!element) return
      const focusable = focusableIn(element)
      const first = focusable[0]
      const last = focusable[focusable.length - 1]
      if (!first || !last) return
      const active = document.activeElement
      if (event.shiftKey && active === first) {
        event.preventDefault()
        last.focus()
      } else if (!event.shiftKey && active === last) {
        event.preventDefault()
        first.focus()
      }
    }
    const onFocusIn = (event: FocusEvent) => {
      const element = overlay.current
      if (!element || element.contains(event.target as Node)) return
      const first = focusableIn(element)[0]
      ;(first ?? element).focus()
    }
    window.addEventListener('keydown', onKeyDown)
    document.addEventListener('focusin', onFocusIn)
    return () => {
      window.removeEventListener('keydown', onKeyDown)
      document.removeEventListener('focusin', onFocusIn)
    }
  }, [expanded])

  // The trigger unmounts while the overlay is open, so opening moves focus
  // into the modal and closing puts it back on the trigger.
  useEffect(() => {
    if (expanded) {
      wasExpanded.current = true
      overlay.current?.focus()
      return
    }
    if (!wasExpanded.current) return
    wasExpanded.current = false
    trigger.current?.focus()
  }, [expanded])

  return (
    <Panel
      title={agentLabel(agent)}
      description={`${agent.agent_kind ?? 'agent'} · ${agent.cwd ?? 'unknown directory'}`}
      status={status}
      actions={
        !expanded && (
          <Button ref={trigger} size="xs" variant="ghost" onClick={() => setExpanded(true)}>
            Full screen
          </Button>
        )
      }
    >
      {/* Always mounted: a live region that appears already filled does not
          announce the change from idle to waiting. */}
      <p className="mb-2 text-xs text-status-waiting empty:hidden" aria-live="polite">
        {blocked ? `Waiting: ${blocked}` : ''}
      </p>
      {session && (
        <dl className="mb-3 flex flex-wrap items-baseline gap-x-4 gap-y-1 text-xs">
          <div className="flex min-w-0 items-baseline gap-1.5">
            <dt className="text-muted-foreground">Claude session</dt>
            <dd className="truncate font-mono" title={session.sessionId}>
              {session.sessionId.slice(0, 8)}
            </dd>
          </div>
          <div className="flex min-w-0 items-baseline gap-1.5">
            <dt className="text-muted-foreground">Model</dt>
            <dd className="truncate">{session.model}</dd>
          </div>
          <div className="flex items-baseline gap-1.5">
            <dt className="text-muted-foreground">Tokens</dt>
            <dd className="tabular-nums">{session.tokens}</dd>
          </div>
          <div className="flex items-baseline gap-1.5">
            <dt className="text-muted-foreground">Est. cost</dt>
            <dd className="tabular-nums">{session.cost}</dd>
          </div>
          {session.waiting && (
            <div className="flex items-baseline gap-1.5">
              <dt className="text-muted-foreground">Waiting for</dt>
              <dd>{session.waiting}</dd>
            </div>
          )}
        </dl>
      )}
      <div className="mb-3 flex flex-wrap gap-1">
        {!question && (
          <>
            <Button
              size="xs"
              variant={blocked ? 'default' : 'ghost'}
              disabled={inFlight('approve') || !blocked}
              title={blocked ? undefined : 'no prompt is waiting'}
              onClick={() => onAct({ kind: 'approve' })}
            >
              Approve
            </Button>
            <Button
              size="xs"
              variant={blocked ? 'outline' : 'ghost'}
              disabled={inFlight('deny') || !blocked}
              onClick={() => onAct({ kind: 'deny' })}
            >
              Deny
            </Button>
          </>
        )}
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
          className="text-muted-foreground hover:text-destructive"
          disabled={inFlight('kill')}
          onClick={() => setConfirmKill(true)}
        >
          Kill
        </Button>
      </div>
      <div
        ref={overlay}
        role={expanded ? 'dialog' : undefined}
        aria-modal={expanded || undefined}
        aria-label={expanded ? 'Expanded agent view' : undefined}
        tabIndex={expanded ? -1 : undefined}
        className={
          expanded
            ? 'fixed inset-2 z-50 flex flex-col gap-2 overscroll-contain rounded-2xl border bg-popover p-4 text-popover-foreground shadow-2xl focus:outline-none'
            : 'flex flex-col'
        }
      >
        {expanded && (
          <div className="flex flex-wrap items-center justify-between gap-2">
            <p className="min-w-0 truncate text-sm font-semibold">{agentLabel(agent)}</p>
            <Button size="xs" variant="ghost" onClick={() => setExpanded(false)}>
              Exit full screen
            </Button>
          </div>
        )}
        <AgentChat
          agent={agent}
          claude={claude}
          busy={busy}
          onAct={onAct}
          className={expanded ? 'h-auto min-h-0 flex-1' : undefined}
        />
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
