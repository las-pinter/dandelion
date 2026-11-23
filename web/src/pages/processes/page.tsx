import { ListTree } from 'lucide-react'
import type { PageDefinition } from '@/app/page-registry'
import { Panel } from '@/components/Panel/Panel'
import { ProcessTable } from '@/components/ProcessTable/ProcessTable'
import { Button } from '@/components/ui/button'
import { useProcesses } from '@/hooks/use-processes'

export function ProcessesPage() {
  const { processes, loading, error, busyPid, refresh, signal, renice } = useProcesses()

  return (
    <Panel
      title="Processes"
      description="Read from /proc. The list refreshes on demand and after every action."
      actions={
        <Button variant="outline" size="sm" onClick={refresh}>
          Refresh
        </Button>
      }
    >
      <ProcessTable
        processes={processes}
        loading={loading}
        error={error}
        busyPid={busyPid}
        onSignal={signal}
        onRenice={renice}
      />
    </Panel>
  )
}

export const page: PageDefinition = {
  path: '/processes',
  title: 'Processes',
  icon: ListTree,
  order: 2,
  Component: ProcessesPage,
}
