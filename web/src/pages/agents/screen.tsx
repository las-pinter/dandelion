import { useRef, useState, type FormEvent } from 'react'
import { AgentDetail } from '@/components/AgentDetail/AgentDetail'
import { AgentList } from '@/components/AgentList/AgentList'
import { Panel } from '@/components/Panel/Panel'
import { TranscriptPanel } from '@/components/TranscriptPanel/TranscriptPanel'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { useAgents } from '@/hooks/use-agents'
import { claudeFor, claudeLabel, unmanagedSessions, type AgentAction } from '@/lib/agents'

export function AgentsPage() {
  const { snapshot, loading, error, busy, refresh, act, spawn } = useAgents()
  const agents = snapshot?.agents ?? []
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const unmanaged = unmanagedSessions(snapshot?.claude)
  const unmanagedSelected =
    selectedId?.startsWith('claude:') === true
      ? (unmanaged.find((entry) => `claude:${entry.sessionId}` === selectedId) ?? null)
      : null
  const selected = unmanagedSelected
    ? null
    : (agents.find((agent) => agent.id === selectedId) ?? agents[0] ?? null)
  const claude = claudeFor(snapshot?.claude, selected?.id)

  const detail = useRef<HTMLDivElement>(null)

  // A selected agent fills the screen: bring its panel to the top so the
  // prompt stays visible instead of below the fold.
  const show = (id: string) => {
    setSelectedId(id)
    const motion =
      typeof window.matchMedia === 'function' &&
      window.matchMedia('(prefers-reduced-motion: reduce)').matches
    requestAnimationFrame(() =>
      detail.current?.scrollIntoView?.({
        block: 'start',
        behavior: motion ? 'auto' : 'smooth',
      }),
    )
  }

  const [line, setLine] = useState('claude')
  const [cwd, setCwd] = useState('')
  const [name, setName] = useState('')
  const [spawning, setSpawning] = useState(false)

  const start = async (event: FormEvent) => {
    event.preventDefault()
    if (line.trim() === '') return
    setSpawning(true)
    const info = await spawn({
      line,
      cwd: cwd.trim() || undefined,
      name: name.trim() || undefined,
    })
    setSpawning(false)
    if (info) show(info.id)
  }

  const actOnSelected = (action: AgentAction) => {
    if (selected) void act(selected.id, action)
  }

  return (
    <div className="flex flex-col gap-4">
      <Panel
        title="Agents"
        description="Live agents from the terminal backend, with their state and pane."
        actions={
          <Button variant="outline" size="sm" onClick={refresh}>
            Refresh
          </Button>
        }
      >
        {error && (
          <p role="alert" className="mb-3 text-sm text-destructive">
            {error}
          </p>
        )}
        {snapshot && !snapshot.available && (
          <p className="mb-3 rounded-lg border border-dashed p-3 text-sm text-muted-foreground">
            No terminal backend is reachable
            {snapshot.backend?.detail ? `: ${snapshot.backend.detail}` : ''}. Start herdr-core to
            list agents.
          </p>
        )}
        {loading && !snapshot ? (
          <p className="py-8 text-center text-sm text-muted-foreground">Reading agents…</p>
        ) : (
          <AgentList
            agents={agents}
            unmanaged={unmanaged}
            selectedId={
              unmanagedSelected ? `claude:${unmanagedSelected.sessionId}` : (selected?.id ?? null)
            }
            onSelect={show}
          />
        )}
      </Panel>

      {/* `html` carries the scroll padding that keeps a scrolled-to element
          clear of the bars, so this wrapper needs no margin of its own. */}
      <div ref={detail} className="flex flex-col gap-4">
        {selected && (
          <AgentDetail
            key={selected.id}
            agent={selected}
            claude={claude}
            busy={busy}
            onAct={actOnSelected}
          />
        )}

        {unmanagedSelected && (
          <Panel
            title={claudeLabel(unmanagedSelected)}
            description={`Unmanaged Claude Code session · ${unmanagedSelected.cwd || 'unknown directory'}`}
          >
            <TranscriptPanel sessionId={unmanagedSelected.sessionId} className="h-[70vh]" />
          </Panel>
        )}
      </div>

      <Panel
        title="Start an agent"
        description="Runs a command in a new terminal at the given directory, usually a worktree."
      >
        <form
          className="flex flex-col gap-2 sm:flex-row sm:items-end"
          onSubmit={(event) => void start(event)}
        >
          <label className="flex-1 text-xs font-medium text-muted-foreground">
            Command line
            <Input
              className="mt-1"
              value={line}
              onChange={(event) => setLine(event.target.value)}
              placeholder="claude"
            />
          </label>
          <label className="flex-1 text-xs font-medium text-muted-foreground">
            Directory
            <Input
              className="mt-1"
              value={cwd}
              onChange={(event) => setCwd(event.target.value)}
              placeholder="/home/dev/worktree"
            />
          </label>
          <label className="flex-1 text-xs font-medium text-muted-foreground">
            Name (optional)
            <Input
              className="mt-1"
              value={name}
              onChange={(event) => setName(event.target.value)}
              placeholder="worktree-agent"
            />
          </label>
          <Button type="submit" size="sm" disabled={spawning || line.trim() === ''}>
            Start
          </Button>
        </form>
      </Panel>
    </div>
  )
}
