import { useId, useState } from 'react'
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
import { StatusPill } from '@/components/StatusPill/StatusPill'
import { XtermPane } from '@/components/XtermPane/XtermPane'
import {
  agentLabel,
  agentSocketUrl,
  blockedLabel,
  uiStatus,
  type AgentAction,
  type AgentInfo,
} from '@/lib/agents'

export interface AgentDetailProps {
  agent: AgentInfo
  /** The action in flight, as `terminal:kind`. */
  busy?: string | null
  onAct: (action: AgentAction) => void
}

/** One agent: its live pane, the prompt box, and the control buttons. */
export function AgentDetail({ agent, busy, onAct }: AgentDetailProps) {
  const [prompt, setPrompt] = useState('')
  const [confirmKill, setConfirmKill] = useState(false)
  const [paneError, setPaneError] = useState<string | null>(null)
  const promptId = useId()
  const status = uiStatus(agent)
  const blocked = blockedLabel(agent.agent_state)
  const inFlight = (kind: string) => busy === `${agent.id}:${kind}`

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
      <div className="h-80 overflow-hidden rounded-lg border bg-background">
        <XtermPane
          url={agentSocketUrl(agent.id)}
          readOnly
          onClosedWithoutOpen={(reason) =>
            setPaneError(reason ?? 'the agent pane could not attach')
          }
        />
      </div>
      {paneError && (
        <p role="alert" className="mt-2 text-xs text-destructive">
          {paneError}
        </p>
      )}

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
