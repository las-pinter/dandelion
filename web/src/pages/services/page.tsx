import { useState } from 'react'
import { Settings2 } from 'lucide-react'
import type { PageDefinition } from '@/app/page-registry'
import { JournalPanel } from '@/components/JournalPanel/JournalPanel'
import { Panel } from '@/components/Panel/Panel'
import { ServiceTable } from '@/components/ServiceTable/ServiceTable'
import { Button } from '@/components/ui/button'
import { useServices } from '@/hooks/use-services'
import type { UnitInfo } from '@/lib/services'

export function ServicesPage() {
  const {
    snapshot,
    loading,
    error,
    busy,
    journal,
    journalLoading,
    journalError,
    refresh,
    act,
    loadJournal,
  } = useServices()
  const [selected, setSelected] = useState<UnitInfo | null>(null)

  const select = (unit: UnitInfo) => {
    setSelected(unit)
    void loadJournal(unit.scope, unit.name)
  }

  const refreshJournal = () => {
    if (selected) void loadJournal(selected.scope, selected.name)
  }

  return (
    <div className="flex flex-col gap-4">
      <Panel
        title="Services"
        description="Units from the user and system managers. System actions need an allowlist rule and the sudoers drop-in."
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
        {loading && !snapshot ? (
          <p className="py-8 text-center text-sm text-muted-foreground">Reading units…</p>
        ) : snapshot ? (
          <ServiceTable
            units={snapshot.units}
            rules={snapshot.allowlist}
            selected={selected}
            busy={busy}
            onSelect={select}
            onAction={(unit, action) => void act(unit.scope, unit.name, action)}
          />
        ) : null}
      </Panel>
      <JournalPanel
        unit={selected}
        entries={journal}
        loading={journalLoading}
        error={journalError}
        onRefresh={refreshJournal}
      />
    </div>
  )
}

export const page: PageDefinition = {
  path: '/services',
  title: 'Services',
  icon: Settings2,
  order: 3,
  Component: ServicesPage,
}
