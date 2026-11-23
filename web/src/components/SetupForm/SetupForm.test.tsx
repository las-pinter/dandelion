import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, expect, it, vi } from 'vitest'
import type { SetupStatus } from '@/lib/auth'
import { SetupForm } from './SetupForm'

function jsonResponse(body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  })
}

afterEach(() => {
  vi.unstubAllGlobals()
})

const almostDone: SetupStatus = { valid: true, has_password: false, has_passkey: true }

it('sets the password and finishes setup', async () => {
  const fetchMock = vi.fn<typeof fetch>(async (input) => {
    if (String(input).includes('/setup/complete')) {
      return new Response(null, { status: 204 })
    }
    return jsonResponse({ valid: true, has_password: true, has_passkey: true })
  })
  vi.stubGlobal('fetch', fetchMock)
  const onComplete = vi.fn<() => void>()
  render(
    <SetupForm token="setup-token" onComplete={onComplete} loadStatus={async () => almostDone} />,
  )

  await userEvent.type(
    await screen.findByLabelText('At least 12 characters'),
    'correct horse battery',
  )
  await userEvent.click(screen.getByRole('button', { name: 'Set password' }))

  expect(await screen.findByText('Password set.')).toBeInTheDocument()
  const finish = screen.getByRole('button', { name: 'Finish setup' })
  expect(finish).toBeEnabled()

  await userEvent.click(finish)

  expect(onComplete).toHaveBeenCalledOnce()
})

it('reports an expired setup link', async () => {
  render(
    <SetupForm
      token="expired"
      loadStatus={async () => ({ valid: false, has_password: false, has_passkey: false })}
    />,
  )

  expect(await screen.findByText('This setup link is invalid or has expired.')).toBeInTheDocument()
})

it('will not finish until both factors exist', async () => {
  render(
    <SetupForm
      token="setup-token"
      loadStatus={async () => ({ valid: true, has_password: false, has_passkey: false })}
    />,
  )

  const finish = await screen.findByRole('button', { name: 'Finish setup' })

  expect(finish).toBeDisabled()
  expect(screen.getByText('Register a passkey and set a password to finish.')).toBeInTheDocument()
})
