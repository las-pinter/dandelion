import { useId, useState, type Ref } from 'react'
import { Button } from '@/components/ui/button'
import { Textarea } from '@/components/ui/textarea'
import type { AgentAction } from '@/lib/agents'

export interface AnswerBarProps {
  /** Whether an action is in flight; the controls stay visible but disabled. */
  busy?: boolean
  /** Receives the answer field, so a caller can move focus into it. */
  inputRef?: Ref<HTMLTextAreaElement>
  onAct: (action: AgentAction) => void
}

/**
 * The controls for a question that blocks the agent.
 *
 * Up and Down move the highlight, Enter accepts it, Escape skips, and a typed
 * answer is forced into the dialog. The typed text is never audited, like
 * every other typed input.
 */
export function AnswerBar({ busy, inputRef, onAct }: AnswerBarProps) {
  const [text, setText] = useState('')
  const fieldId = useId()

  const send = () => {
    if (text.trim() === '') return
    onAct({ kind: 'answer', text })
    setText('')
  }

  return (
    <form
      aria-label="Answer the question"
      className="flex shrink-0 flex-col gap-2 rounded-xl border border-status-waiting/40 bg-status-waiting/5 p-2.5"
      onSubmit={(event) => {
        event.preventDefault()
        send()
      }}
    >
      <div className="flex flex-wrap items-center gap-1">
        <p className="mr-1 text-xs text-status-waiting">The agent asks a question.</p>
        <Button
          type="button"
          size="xs"
          variant="outline"
          disabled={busy}
          onClick={() => onAct({ kind: 'keys', keys: ['up'] })}
        >
          Up
        </Button>
        <Button
          type="button"
          size="xs"
          variant="outline"
          disabled={busy}
          onClick={() => onAct({ kind: 'keys', keys: ['down'] })}
        >
          Down
        </Button>
        <Button type="button" size="xs" disabled={busy} onClick={() => onAct({ kind: 'approve' })}>
          Accept (Enter)
        </Button>
        <Button
          type="button"
          size="xs"
          variant="outline"
          disabled={busy}
          onClick={() => onAct({ kind: 'deny' })}
        >
          Skip (Escape)
        </Button>
      </div>
      <div className="flex items-end gap-2 rounded-xl border bg-background/60 p-1.5 transition-colors focus-within:border-ring focus-within:ring-3 focus-within:ring-ring/30">
        <label htmlFor={fieldId} className="sr-only">
          Answer
        </label>
        <Textarea
          ref={inputRef}
          id={fieldId}
          rows={1}
          value={text}
          onChange={(event) => setText(event.target.value)}
          onKeyDown={(event) => {
            if (event.key !== 'Enter' || event.shiftKey || event.nativeEvent.isComposing) return
            event.preventDefault()
            send()
          }}
          placeholder="Type an answer into the question"
          className="max-h-40 min-h-9 flex-1 resize-none border-0 bg-transparent py-1.5 shadow-none focus-visible:ring-0"
        />
        <Button type="submit" size="sm" disabled={busy || text.trim() === ''}>
          Answer
        </Button>
      </div>
    </form>
  )
}
