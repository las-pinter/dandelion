import { useEffect, useId, useRef, useState } from 'react'
import { AnswerBar } from '@/components/AnswerBar/AnswerBar'
import { TranscriptPanel } from '@/components/TranscriptPanel/TranscriptPanel'
import { XtermPane } from '@/components/XtermPane/XtermPane'
import { Button } from '@/components/ui/button'
import { Textarea } from '@/components/ui/textarea'
import {
  agentSocketUrl,
  asksQuestion,
  showLiveStrip,
  type AgentAction,
  type AgentInfo,
  type ClaudeAgent,
} from '@/lib/agents'
import { cn } from '@/lib/utils'

export interface AgentChatProps {
  agent: AgentInfo
  /** The Claude Code session behind this agent, when one was found. */
  claude?: ClaudeAgent | null
  /** The action in flight, as `terminal:kind`. */
  busy?: string | null
  onAct: (action: AgentAction) => void
  /** Layout classes; the expanded overlay fills its parent. */
  className?: string
}

/**
 * One agent as a conversation: the transcript above, the live terminal strip
 * where the turn happens, the prompt at the bottom.
 *
 * The strip appears with the turn and leaves when the agent settles; the owner
 * can show or hide it at any time. A question block swaps the prompt box for
 * the answer bar so the dialog can be answered, not only accepted.
 */
export function AgentChat({ agent, claude, busy, onAct, className }: AgentChatProps) {
  const [prompt, setPrompt] = useState('')
  const [choice, setChoice] = useState<'auto' | 'shown' | 'hidden'>('auto')
  // A failed pane stays dead until it remounts: bumping this reconnects it.
  const [stripAttempt, setStripAttempt] = useState(0)
  const [paneError, setPaneError] = useState<string | null>(null)
  const promptRef = useRef<HTMLTextAreaElement>(null)
  const answerRef = useRef<HTMLTextAreaElement>(null)
  const promptId = useId()
  const state = agent.agent_state?.state
  const question = asksQuestion(agent.agent_state)
  const hadQuestion = useRef(question)
  const hasTranscript = claude != null
  const stripShown = !hasTranscript || showLiveStrip(agent.agent_state, choice)
  const inFlight = (kind: string) => busy === `${agent.id}:${kind}`
  const agentBusy = busy?.startsWith(`${agent.id}:`) === true

  // A failed pane is replaced at the next turn, so the turn it watches is not
  // invisible; a healthy pane is left alone, because reattaching repaints it.
  const reconnectIfFailed = () => {
    if (paneError === null) return
    setPaneError(null)
    setStripAttempt((attempt) => attempt + 1)
  }

  // Any state change returns the strip to automatic: a new turn or a block
  // opens it, a settle closes it. Adjusted during render, the documented React
  // form of "reset some state when a prop changes": an effect here would
  // render twice.
  const [lastState, setLastState] = useState(state)
  if (state !== lastState) {
    setLastState(state)
    if (state !== 'working') setChoice('auto')
    reconnectIfFailed()
  }

  // The prompt and the answer bar take each other's place, so focus follows.
  useEffect(() => {
    if (question === hadQuestion.current) return
    hadQuestion.current = question
    if (question) answerRef.current?.focus()
    else promptRef.current?.focus()
  }, [question])

  const send = () => {
    if (prompt.trim() === '') return
    onAct({ kind: 'prompt', text: prompt })
    setPrompt('')
    // The turn starts now, so the strip opens with it rather than waiting for
    // the first state refresh.
    setChoice('shown')
    reconnectIfFailed()
  }

  return (
    <div className={cn('flex h-[80dvh] min-h-[32rem] flex-col gap-2', className)}>
      <div className="flex min-h-0 flex-1 flex-col">
        {hasTranscript && claude ? (
          <TranscriptPanel sessionId={claude.sessionId} className="h-full" />
        ) : (
          <p className="rounded-lg border border-dashed p-3 text-sm text-muted-foreground">
            No Claude Code session is behind this agent; the live terminal is the whole view.
          </p>
        )}
      </div>

      {hasTranscript && (
        <div className="mt-1 flex items-center gap-2">
          <span
            aria-hidden
            className={cn(
              'size-1.5 rounded-full',
              state === 'working'
                ? 'animate-pulse bg-status-working'
                : stripShown
                  ? 'bg-status-idle'
                  : 'bg-muted-foreground/40',
            )}
          />
          <p className="text-xs text-muted-foreground">Live output</p>
          <span className="flex-1" />
          <Button
            size="xs"
            variant="ghost"
            onClick={() => {
              setPaneError(null)
              setChoice(stripShown ? 'hidden' : 'shown')
            }}
          >
            {stripShown ? 'Hide terminal' : 'Show terminal'}
          </Button>
        </div>
      )}

      {stripShown && (
        <>
          <div
            className={cn(
              'overflow-hidden rounded-xl bg-background ring-1 ring-border/60',
              hasTranscript ? 'min-h-[280px] basis-[40%]' : 'min-h-0 flex-1',
            )}
          >
            <XtermPane
              key={stripAttempt}
              url={agentSocketUrl(agent.id)}
              readOnly
              onClosedWithoutOpen={(reason) =>
                setPaneError(reason ?? 'the agent pane could not attach')
              }
            />
          </div>
          {paneError && (
            <p role="alert" className="text-xs text-destructive">
              {paneError}
            </p>
          )}
        </>
      )}

      {question ? (
        <AnswerBar
          busy={agentBusy}
          inputRef={answerRef}
          onAct={(action) => {
            setChoice('auto')
            onAct(action)
          }}
        />
      ) : (
        <form
          className="flex items-end gap-2 rounded-xl border bg-background/60 p-1.5 transition-colors focus-within:border-ring focus-within:ring-3 focus-within:ring-ring/30"
          onSubmit={(event) => {
            event.preventDefault()
            send()
          }}
        >
          <label htmlFor={promptId} className="sr-only">
            Prompt
          </label>
          <Textarea
            ref={promptRef}
            id={promptId}
            rows={1}
            value={prompt}
            onChange={(event) => setPrompt(event.target.value)}
            onKeyDown={(event) => {
              if (event.key !== 'Enter' || event.shiftKey || event.nativeEvent.isComposing) return
              event.preventDefault()
              send()
            }}
            placeholder="Send a prompt…"
            className="max-h-40 min-h-9 flex-1 resize-none border-0 bg-transparent py-1.5 shadow-none focus-visible:ring-0"
          />
          <Button type="submit" size="sm" disabled={inFlight('prompt') || prompt.trim() === ''}>
            Send
          </Button>
        </form>
      )}
    </div>
  )
}
