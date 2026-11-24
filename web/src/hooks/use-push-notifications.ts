import { useCallback, useEffect, useState } from 'react'
import { api } from '@/api/client'
import { messageOf } from '@/lib/api-error'
import { pushSupported, urlBase64ToBytes } from '@/lib/push'

export type PushStatus =
  /** This browser has no service worker or push support. */
  | 'unsupported'
  /** Supported, not subscribed, and permission was never asked. */
  | 'default'
  /** The browser blocked notifications for this site. */
  | 'denied'
  /** This device has a live subscription. */
  | 'subscribed'
  /** A request is in flight. */
  | 'working'
  /** The last attempt failed; see `error`. */
  | 'error'

export interface PushNotificationsState {
  status: PushStatus
  /** The newest enable, disable, or test problem. */
  error: string | null
  /** Registers the worker, asks permission, and stores the subscription. */
  enable: () => Promise<void>
  /** Forgets the subscription here and on the server. */
  disable: () => Promise<void>
  /** Sends a test notification to every subscribed device. */
  test: () => Promise<void>
}

/**
 * Subscribes this browser, replacing a subscription made with an older
 * server key: the browser then refuses until the stale one is removed.
 */
async function subscribeToPush(
  registration: ServiceWorkerRegistration,
  publicKey: string,
): Promise<PushSubscription> {
  const options: PushSubscriptionOptionsInit = {
    userVisibleOnly: true,
    applicationServerKey: urlBase64ToBytes(publicKey),
  }
  try {
    return await registration.pushManager.subscribe(options)
  } catch (cause: unknown) {
    const invalidState = cause instanceof Error && cause.name === 'InvalidStateError'
    if (!invalidState) throw cause
    const existing = await registration.pushManager.getSubscription()
    if (!existing) throw cause
    await existing.unsubscribe()
    return await registration.pushManager.subscribe(options)
  }
}

/**
 * Turns this device into a push target for the server.
 *
 * The hook registers `/sw.js` only when notifications are enabled, so a
 * browser that never wants them pays nothing.
 */
export function usePushNotifications(): PushNotificationsState {
  const [status, setStatus] = useState<PushStatus>(() => {
    if (!pushSupported()) return 'unsupported'
    if (Notification.permission === 'denied') return 'denied'
    return 'default'
  })
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (!pushSupported() || Notification.permission === 'denied') return
    let disposed = false
    void (async () => {
      try {
        const registration = await navigator.serviceWorker.getRegistration()
        const subscription = registration ? await registration.pushManager.getSubscription() : null
        // A user action may have moved the status while this check ran; only
        // the untouched initial state may be refined.
        if (!disposed) {
          setStatus((current) =>
            current === 'default' ? (subscription ? 'subscribed' : 'default') : current,
          )
        }
      } catch {
        // A browser that cannot answer leaves the panel in its initial state.
      }
    })()
    return () => {
      disposed = true
    }
  }, [])

  const enable = useCallback(async () => {
    setError(null)
    if (!pushSupported()) {
      setStatus('unsupported')
      return
    }
    setStatus('working')
    try {
      const permission =
        Notification.permission === 'granted' ? 'granted' : await Notification.requestPermission()
      if (permission === 'denied') {
        setStatus('denied')
        return
      }
      if (permission !== 'granted') {
        // The prompt was dismissed, not blocked; it can be asked again.
        setStatus('default')
        return
      }

      await navigator.serviceWorker.register('/sw.js')
      // The new worker must be active before it can own a subscription.
      const registration = await navigator.serviceWorker.ready
      const { data, error: keyError } = await api.GET('/api/notifications/key')
      if (!data) {
        setError(messageOf(keyError, 'the notification key is unavailable'))
        setStatus('error')
        return
      }

      const subscription = await subscribeToPush(registration, data.publicKey)
      const json = subscription.toJSON()
      const { error: failure } = await api.POST('/api/notifications/subscribe', {
        body: {
          endpoint: json.endpoint ?? subscription.endpoint,
          keys: {
            p256dh: json.keys?.p256dh ?? '',
            auth: json.keys?.auth ?? '',
          },
        },
      })
      if (failure !== undefined) {
        await subscription.unsubscribe()
        setError(messageOf(failure, 'the subscription was refused'))
        setStatus('error')
        return
      }
      setStatus('subscribed')
    } catch (cause: unknown) {
      setError(cause instanceof Error ? cause.message : 'notifications could not be enabled')
      setStatus('error')
    }
  }, [])

  const disable = useCallback(async () => {
    setError(null)
    setStatus('working')
    try {
      const registration = await navigator.serviceWorker.getRegistration()
      const subscription = registration ? await registration.pushManager.getSubscription() : null
      if (subscription) {
        const { error: failure } = await api.POST('/api/notifications/unsubscribe', {
          body: { endpoint: subscription.endpoint },
        })
        if (failure !== undefined) {
          // Keep the local subscription so a retry can remove both halves.
          setError(messageOf(failure, 'the subscription was not removed'))
          setStatus('error')
          return
        }
        await subscription.unsubscribe()
      }
      setStatus('default')
    } catch (cause: unknown) {
      setError(cause instanceof Error ? cause.message : 'notifications could not be disabled')
      setStatus('error')
    }
  }, [])

  const test = useCallback(async () => {
    setError(null)
    try {
      const { data, error: failure } = await api.POST('/api/notifications/test')
      if (failure !== undefined) {
        setError(messageOf(failure, 'the test was not sent'))
      } else if ((data?.delivered ?? 0) === 0) {
        setError('no subscribed device accepted the test notification')
      }
    } catch (cause: unknown) {
      setError(cause instanceof Error ? cause.message : 'the test was not sent')
    }
  }, [])

  return { status, error, enable, disable, test }
}
