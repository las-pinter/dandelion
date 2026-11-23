import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, expect, it, vi } from 'vitest'
import { LoginForm } from './LoginForm'

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  })
}

afterEach(() => {
  vi.unstubAllGlobals()
})

it('signs in with the owner password', async () => {
  const fetchMock = vi.fn<typeof fetch>(async () => jsonResponse({ authenticated: true }))
  vi.stubGlobal('fetch', fetchMock)
  const onSuccess = vi.fn<() => void>()
  render(<LoginForm onSuccess={onSuccess} />)

  await userEvent.type(screen.getByLabelText('Password'), 'correct horse battery')
  await userEvent.click(screen.getByRole('button', { name: 'Sign in' }))

  expect(onSuccess).toHaveBeenCalledOnce()
  const [url, init] = fetchMock.mock.calls[0] ?? []
  expect(url).toBe('/api/auth/login/password')
  expect(init).toMatchObject({ method: 'POST', credentials: 'same-origin' })
})

it('shows the server message when the password is wrong', async () => {
  vi.stubGlobal(
    'fetch',
    vi.fn<typeof fetch>(async () =>
      jsonResponse({ error: 'invalid_credentials', message: 'The password is incorrect.' }, 401),
    ),
  )
  const onSuccess = vi.fn<() => void>()
  render(<LoginForm onSuccess={onSuccess} />)

  await userEvent.type(screen.getByLabelText('Password'), 'wrong password here')
  await userEvent.click(screen.getByRole('button', { name: 'Sign in' }))

  expect(await screen.findByRole('alert')).toHaveTextContent('The password is incorrect.')
  expect(onSuccess).not.toHaveBeenCalled()
})

it('requires a password before calling the server', async () => {
  const fetchMock = vi.fn<typeof fetch>()
  vi.stubGlobal('fetch', fetchMock)
  render(<LoginForm onSuccess={vi.fn<() => void>()} />)

  await userEvent.click(screen.getByRole('button', { name: 'Sign in' }))

  expect(await screen.findByRole('alert')).toHaveTextContent('Enter the password.')
  expect(fetchMock).not.toHaveBeenCalled()
})
