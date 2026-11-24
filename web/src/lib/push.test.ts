import { afterEach, describe, expect, it, vi } from 'vitest'
import { pushSupported, urlBase64ToBytes } from './push'

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('push helpers', () => {
  it('decodes a base64url key into bytes', () => {
    expect([...urlBase64ToBytes('AQID')]).toEqual([1, 2, 3])
    expect([...urlBase64ToBytes('_w')]).toEqual([255])
    expect([...urlBase64ToBytes('AQIDBA')]).toEqual([1, 2, 3, 4])
  })

  it('reports an unsupported browser', () => {
    expect(pushSupported()).toBe(false)
  })

  it('accepts a browser with a worker, push, and notifications', () => {
    Object.defineProperty(navigator, 'serviceWorker', { value: {}, configurable: true })
    vi.stubGlobal('PushManager', {})
    vi.stubGlobal('Notification', {})

    expect(pushSupported()).toBe(true)

    Reflect.deleteProperty(navigator, 'serviceWorker')
  })
})
