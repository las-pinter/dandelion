import { afterEach, describe, expect, it, vi } from 'vitest'
import { registerServiceWorker } from './pwa'

function stubServiceWorker(register: () => Promise<unknown>) {
  Object.defineProperty(navigator, 'serviceWorker', {
    configurable: true,
    value: { register: vi.fn<(path: string) => Promise<unknown>>(register) },
  })
}

afterEach(() => {
  Reflect.deleteProperty(navigator, 'serviceWorker')
  vi.restoreAllMocks()
})

describe('registerServiceWorker', () => {
  it('registers the lifecycle worker', () => {
    stubServiceWorker(() => Promise.resolve())

    registerServiceWorker()

    expect(navigator.serviceWorker.register).toHaveBeenCalledWith('/sw.js')
  })

  it('does nothing without the service worker API', () => {
    expect(() => registerServiceWorker()).not.toThrow()
  })

  it('does not throw and warns when registration fails', async () => {
    stubServiceWorker(() => Promise.reject(new Error('no worker for you')))
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined)

    expect(() => registerServiceWorker()).not.toThrow()
    await Promise.resolve()

    expect(warn).toHaveBeenCalled()
  })
})
