import { Bell, BellOff } from 'lucide-react'
import { Panel } from '@/components/Panel/Panel'
import { Button } from '@/components/ui/button'
import { usePushNotifications } from '@/hooks/use-push-notifications'

/** Turns this browser into a push target for the server. */
export function NotificationsPanel() {
  const { status, error, enable, disable, test } = usePushNotifications()
  const busy = status === 'working'

  return (
    <Panel
      title="Notifications"
      description="Push alerts on this device when an agent needs you or finishes, a unit fails, or a resource stays high."
    >
      {error && (
        <p role="alert" className="mb-3 text-sm text-destructive">
          {error}
        </p>
      )}
      {status === 'unsupported' && (
        <p className="text-sm text-muted-foreground">
          This browser cannot receive push notifications. Use the HTTPS address or install the app.
        </p>
      )}
      {status === 'denied' && (
        <p className="text-sm text-muted-foreground">
          Notifications are blocked for this site. Allow them in the browser settings, then reload.
        </p>
      )}
      {(status === 'default' || status === 'error') && (
        <div className="flex flex-wrap items-center gap-2">
          <p className="min-w-48 flex-1 text-sm text-muted-foreground">
            Each subscribed device gets a push when an agent needs you or finishes.
          </p>
          <Button size="sm" disabled={busy} onClick={() => void enable()}>
            <Bell className="mr-1 size-4" aria-hidden />
            Enable
          </Button>
          {status === 'error' && (
            <Button size="sm" variant="ghost" disabled={busy} onClick={() => void disable()}>
              <BellOff className="mr-1 size-4" aria-hidden />
              Disable
            </Button>
          )}
        </div>
      )}
      {status === 'working' && (
        <p role="status" className="text-sm text-muted-foreground">
          Working…
        </p>
      )}
      {status === 'subscribed' && (
        <div className="flex flex-wrap items-center gap-2">
          <p role="status" className="min-w-48 flex-1 text-sm text-muted-foreground">
            This device is subscribed.
          </p>
          <Button size="sm" variant="outline" disabled={busy} onClick={() => void test()}>
            Send test
          </Button>
          <Button size="sm" variant="ghost" disabled={busy} onClick={() => void disable()}>
            <BellOff className="mr-1 size-4" aria-hidden />
            Disable
          </Button>
        </div>
      )}
    </Panel>
  )
}
