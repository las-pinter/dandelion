import { render, screen } from '@testing-library/react'
import { expect, it } from 'vitest'
import { RequireAuth } from './RequireAuth'

it('renders the app when a session exists', async () => {
  render(
    <RequireAuth loadStatus={async () => ({ setup_required: false, authenticated: true })}>
      <p>The signed-in app</p>
    </RequireAuth>,
  )

  expect(await screen.findByText('The signed-in app')).toBeInTheDocument()
})

it('shows a retry action when the status cannot be read', async () => {
  render(
    <RequireAuth loadStatus={async () => Promise.reject(new Error('server is down'))}>
      <p>never</p>
    </RequireAuth>,
  )

  expect(await screen.findByRole('alert')).toHaveTextContent('server is down')
  expect(screen.getByRole('button', { name: 'Try again' })).toBeInTheDocument()
})
