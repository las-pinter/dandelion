import { act, renderHook, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { usePushNotifications } from './use-push-notifications'

const { get, post } = vi.hoisted(() => ({
  get: vi.fn<(path: string, options?: unknown) => Promise<{ data?: unknown; error?: unknown }>>(),
  post: vi.fn<(path: string, options?: unknown) => Promise<{ data?: unknown; error?: unknown }>>(),
}))

vi.mock('@/api/client', () => ({ api: { GET: get, POST: post } }))

class FakeSubscription {
  readonly endpoint = 'https://push.example/send/abc'
  readonly unsubscribe = vi.fn<() => Promise<boolean>>(async () => true)

  toJSON() {
    return { endpoint: this.endpoint, keys: { p256dh: 'p256dh-key', auth: 'auth-key' } }
  }
}

const registration = {
  pushManager: {
    subscribe: vi.fn<(options: unknown) => Promise<FakeSubscription>>(
      async (_options: unknown) => new FakeSubscription(),
    ),
    getSubscription: vi.fn<() => Promise<FakeSubscription | null>>(async () => null),
  },
}
const serviceWorker = {
  register: vi.fn<(url: string) => Promise<typeof registration>>(
    async (_url: string) => registration,
  ),
  getRegistration: vi.fn<() => Promise<typeof registration>>(async () => registration),
  ready: Promise.resolve(registration),
}

const fakeNotification = {
  permission: 'default' as NotificationPermission,
  requestPermission: vi.fn<() => Promise<NotificationPermission>>(
    async () => 'granted' as NotificationPermission,
  ),
}

let descriptor: PropertyDescriptor | undefined

function installBrowser() {
  descriptor = Object.getOwnPropertyDescriptor(navigator, 'serviceWorker')
  Object.defineProperty(navigator, 'serviceWorker', { value: serviceWorker, configurable: true })
  vi.stubGlobal('PushManager', {})
  vi.stubGlobal('Notification', fakeNotification)
}

beforeEach(() => {
  get.mockResolvedValue({ data: { publicKey: 'AQID' } })
  post.mockResolvedValue({ data: { delivered: 1 } })
  registration.pushManager.subscribe.mockReset()
  registration.pushManager.subscribe.mockImplementation(async () => new FakeSubscription())
  registration.pushManager.getSubscription.mockReset()
  registration.pushManager.getSubscription.mockResolvedValue(null)
  serviceWorker.register.mockClear()
  serviceWorker.getRegistration.mockClear()
  fakeNotification.permission = 'default'
  fakeNotification.requestPermission.mockReset()
  fakeNotification.requestPermission.mockResolvedValue('granted')
})

afterEach(() => {
  if (descriptor) Object.defineProperty(navigator, 'serviceWorker', descriptor)
  else Reflect.deleteProperty(navigator, 'serviceWorker')
  descriptor = undefined
  vi.unstubAllGlobals()
  get.mockReset()
  post.mockReset()
})

describe('usePushNotifications', () => {
  it('reports an unsupported browser', () => {
    const { result } = renderHook(() => usePushNotifications())

    expect(result.current.status).toBe('unsupported')
  })

  it('registers, subscribes, and stores the subscription', async () => {
    installBrowser()
    const { result } = renderHook(() => usePushNotifications())

    await act(async () => {
      await result.current.enable()
    })

    expect(serviceWorker.register).toHaveBeenCalledWith('/sw.js')
    expect(get).toHaveBeenCalledWith('/api/notifications/key')
    expect(registration.pushManager.subscribe).toHaveBeenCalledWith({
      userVisibleOnly: true,
      applicationServerKey: new Uint8Array([1, 2, 3]),
    })
    expect(post).toHaveBeenCalledWith('/api/notifications/subscribe', {
      body: {
        endpoint: 'https://push.example/send/abc',
        keys: { p256dh: 'p256dh-key', auth: 'auth-key' },
      },
    })
    expect(result.current.status).toBe('subscribed')
  })

  it('notices an existing subscription on mount', async () => {
    installBrowser()
    registration.pushManager.getSubscription.mockResolvedValue(new FakeSubscription())

    const { result } = renderHook(() => usePushNotifications())

    await waitFor(() => expect(result.current.status).toBe('subscribed'))
  })

  it('stops when the browser denies permission', async () => {
    installBrowser()
    fakeNotification.requestPermission.mockResolvedValue('denied')

    const { result } = renderHook(() => usePushNotifications())
    await act(async () => {
      await result.current.enable()
    })

    expect(result.current.status).toBe('denied')
    expect(serviceWorker.register).not.toHaveBeenCalled()
  })

  it('reports a dismissed permission prompt as undecided', async () => {
    installBrowser()
    fakeNotification.requestPermission.mockResolvedValue('default' as NotificationPermission)

    const { result } = renderHook(() => usePushNotifications())
    await act(async () => {
      await result.current.enable()
    })

    expect(result.current.status).toBe('default')
    expect(serviceWorker.register).not.toHaveBeenCalled()
  })

  it('reports a refused subscription and unsubscribes locally', async () => {
    installBrowser()
    post.mockResolvedValue({ error: { message: 'the subscription was refused' } })
    const subscription = new FakeSubscription()
    registration.pushManager.subscribe.mockResolvedValue(subscription)

    const { result } = renderHook(() => usePushNotifications())
    await act(async () => {
      await result.current.enable()
    })

    expect(result.current.status).toBe('error')
    expect(result.current.error).toContain('refused')
    expect(subscription.unsubscribe).toHaveBeenCalled()
  })

  it('replaces a subscription made with an older server key', async () => {
    installBrowser()
    const stale = new FakeSubscription()
    registration.pushManager.getSubscription.mockResolvedValue(stale)
    const invalidState = Object.assign(new Error('stale key'), { name: 'InvalidStateError' })
    registration.pushManager.subscribe
      .mockRejectedValueOnce(invalidState)
      .mockResolvedValueOnce(new FakeSubscription())

    const { result } = renderHook(() => usePushNotifications())
    await act(async () => {
      await result.current.enable()
    })

    expect(stale.unsubscribe).toHaveBeenCalled()
    expect(registration.pushManager.subscribe).toHaveBeenCalledTimes(2)
    expect(result.current.status).toBe('subscribed')
  })

  it('forgets the subscription on disable', async () => {
    installBrowser()
    const subscription = new FakeSubscription()
    registration.pushManager.getSubscription.mockResolvedValue(subscription)

    const { result } = renderHook(() => usePushNotifications())
    await waitFor(() => expect(result.current.status).toBe('subscribed'))

    await act(async () => {
      await result.current.disable()
    })

    expect(post).toHaveBeenCalledWith('/api/notifications/unsubscribe', {
      body: { endpoint: 'https://push.example/send/abc' },
    })
    expect(subscription.unsubscribe).toHaveBeenCalled()
    expect(result.current.status).toBe('default')
  })

  it('keeps the subscription when the server cannot forget it', async () => {
    installBrowser()
    const subscription = new FakeSubscription()
    registration.pushManager.getSubscription.mockResolvedValue(subscription)
    post.mockResolvedValue({ error: { message: 'the server refused' } })

    const { result } = renderHook(() => usePushNotifications())
    await waitFor(() => expect(result.current.status).toBe('subscribed'))

    await act(async () => {
      await result.current.disable()
    })

    expect(result.current.status).toBe('error')
    expect(result.current.error).toContain('refused')
    expect(subscription.unsubscribe).not.toHaveBeenCalled()
  })

  it('sends a test notification', async () => {
    installBrowser()
    const { result } = renderHook(() => usePushNotifications())

    await act(async () => {
      await result.current.test()
    })

    expect(post).toHaveBeenCalledWith('/api/notifications/test')
    expect(result.current.error).toBeNull()
  })

  it('reports a test no device received', async () => {
    installBrowser()
    post.mockResolvedValue({ data: { delivered: 0 } })
    const { result } = renderHook(() => usePushNotifications())

    await act(async () => {
      await result.current.test()
    })

    expect(result.current.error).toContain('no subscribed device')
  })
})
