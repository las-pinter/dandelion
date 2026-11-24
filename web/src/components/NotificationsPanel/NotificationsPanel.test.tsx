import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import type { PushNotificationsState } from '@/hooks/use-push-notifications'
import { NotificationsPanel } from './NotificationsPanel'

const { usePushNotifications } = vi.hoisted(() => ({
  usePushNotifications: vi.fn<() => PushNotificationsState>(),
}))

vi.mock('@/hooks/use-push-notifications', () => ({ usePushNotifications }))

function state(overrides: Partial<PushNotificationsState> = {}): PushNotificationsState {
  return {
    status: 'default',
    error: null,
    enable: async () => undefined,
    disable: async () => undefined,
    test: async () => undefined,
    ...overrides,
  }
}

describe('NotificationsPanel', () => {
  it('offers to enable notifications', async () => {
    const user = userEvent.setup()
    const enable = vi.fn<() => Promise<void>>(async () => undefined)
    usePushNotifications.mockReturnValue(state({ enable }))

    render(<NotificationsPanel />)
    await user.click(screen.getByRole('button', { name: 'Enable' }))

    expect(enable).toHaveBeenCalledTimes(1)
  })

  it('sends a test and disables a subscribed device', async () => {
    const user = userEvent.setup()
    const test = vi.fn<() => Promise<void>>(async () => undefined)
    const disable = vi.fn<() => Promise<void>>(async () => undefined)
    usePushNotifications.mockReturnValue(state({ status: 'subscribed', test, disable }))

    render(<NotificationsPanel />)
    await user.click(screen.getByRole('button', { name: 'Send test' }))
    await user.click(screen.getByRole('button', { name: 'Disable' }))

    expect(test).toHaveBeenCalledTimes(1)
    expect(disable).toHaveBeenCalledTimes(1)
    expect(screen.getByText('This device is subscribed.')).toBeInTheDocument()
  })

  it('explains each blocked state', () => {
    usePushNotifications.mockReturnValue(state({ status: 'unsupported' }))
    const { rerender } = render(<NotificationsPanel />)
    expect(screen.getByText(/cannot receive push notifications/)).toBeInTheDocument()

    usePushNotifications.mockReturnValue(state({ status: 'denied' }))
    rerender(<NotificationsPanel />)
    expect(screen.getByText(/blocked for this site/)).toBeInTheDocument()
  })

  it('shows the last error', () => {
    usePushNotifications.mockReturnValue(
      state({ status: 'error', error: 'the key is unavailable' }),
    )

    render(<NotificationsPanel />)

    expect(screen.getByRole('alert')).toHaveTextContent('the key is unavailable')
    expect(screen.getByRole('button', { name: 'Enable' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Disable' })).toBeInTheDocument()
  })
})
