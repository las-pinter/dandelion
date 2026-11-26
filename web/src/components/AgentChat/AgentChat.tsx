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

/** The heights of the bars that overlay the page, so they count as chrome. */
function chromeHeights() {
  const height = (name: string) => {
    const node = document.querySelector(`[data-app-chrome="${name}"]`)
    return node ? node.getBoundingClientRect().height : 0
  }
  return { top: height('top'), bottom: height('tab') }
}

/** Focuses a field without scrolling to it when the bars do not hide it. */
function focusField(element: HTMLTextAreaElement | null) {
  if (!element) return
  const box = element.getBoundingClientRect()
  const { top, bottom } = chromeHeights()
  // The visual viewport is what the thumb and the keyboard leave; the layout
  // viewport is the fallback where there is none.
  const view = window.visualViewport
  const pageTop = view ? Math.max(top, view.offsetTop) : top
  const pageBottom = view
    ? Math.min(window.innerHeight, view.offsetTop + view.height)
    : window.innerHeight
  const visible = box.top >= pageTop && box.bottom <= pageBottom - bottom
  element.focus({ preventScroll: visible })
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
  // Focusing must not scroll a page that already shows the field: on a phone
  // the chat is sized to fit, and a plain focus() dragged the page down to the
  // composer on every swap. When the field really is off screen (a viewport
  // too short for the chat), the scroll is the point, so it is left alone.
  useEffect(() => {
    if (question === hadQuestion.current) return
    hadQuestion.current = question
    focusField(question ? answerRef.current : promptRef.current)
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
        <div className="mt-1 flex shrink-0 items-center gap-2">
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
              // About 40 % of the chat, with a 280 px floor from `md` up. A
              // phone cannot spare that floor without starving the transcript,
              // so there the share alone rules. The composer and the rows
              // around it never shrink at all.
              hasTranscript ? 'min-h-[280px] basis-[40%] max-md:min-h-0' : 'min-h-0 flex-1',
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
          className="flex shrink-0 items-end gap-2 rounded-xl border bg-background/60 p-1.5 transition-colors focus-within:border-ring focus-within:ring-3 focus-within:ring-ring/30"
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
