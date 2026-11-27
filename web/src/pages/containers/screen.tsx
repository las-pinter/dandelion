import { useState } from 'react'
import { ContainerLogs } from '@/components/ContainerLogs/ContainerLogs'
import { ContainerTable } from '@/components/ContainerTable/ContainerTable'
import { Panel } from '@/components/Panel/Panel'
import { Button } from '@/components/ui/button'
import { useContainers } from '@/hooks/use-containers'
import { containerLabel, type ContainerInfo } from '@/lib/containers'

export function ContainersPage() {
  const { snapshot, loading, error, busy, logs, logsLoading, logsError, refresh, act, loadLogs } =
    useContainers()
  const [selected, setSelected] = useState<ContainerInfo | null>(null)

  const select = (container: ContainerInfo) => {
    setSelected(container)
    void loadLogs(containerLabel(container))
  }

  const refreshLogs = () => {
    if (selected) void loadLogs(containerLabel(selected))
  }

  const available = snapshot?.availability.available ?? false

  return (
    <div className="flex flex-col gap-4">
      <Panel
        title="Containers"
        description="Docker containers on this machine. Start, stop, and restart run through the local Docker socket."
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
        {snapshot && !available && (
          <p className="mb-3 rounded-lg border border-dashed p-3 text-sm text-muted-foreground">
            Docker is unavailable
            {snapshot.availability.reason ? `: ${snapshot.availability.reason}` : ''}. Container
            actions are disabled.
          </p>
        )}
        {loading && !snapshot ? (
          <p className="py-8 text-center text-sm text-muted-foreground">Reading containers…</p>
        ) : snapshot ? (
          <ContainerTable
            containers={snapshot.containers}
            selected={selected}
            busy={busy}
            onSelect={select}
            onAction={(container, action) => void act(containerLabel(container), action)}
          />
        ) : null}
      </Panel>
      <ContainerLogs
        container={selected}
        lines={logs}
        loading={logsLoading}
        error={logsError}
        onRefresh={refreshLogs}
      />
    </div>
  )
}
