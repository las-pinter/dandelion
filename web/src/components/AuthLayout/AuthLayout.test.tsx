import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { AuthLayout } from './AuthLayout'

describe('AuthLayout', () => {
  it('names the screen and shows the children', () => {
    render(
      <AuthLayout title="Sign in" description="Use a passkey or the owner password.">
        <button type="button">Continue</button>
      </AuthLayout>,
    )

    expect(screen.getByRole('heading', { level: 1, name: 'Sign in' })).toBeInTheDocument()
    expect(screen.getByText('Use a passkey or the owner password.')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Continue' })).toBeInTheDocument()
  })

  it('omits the description when there is none', () => {
    render(
      <AuthLayout title="Set up Dandelion">
        <p>Body</p>
      </AuthLayout>,
    )

    expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent('Set up Dandelion')
    expect(screen.queryByText('Use a passkey')).not.toBeInTheDocument()
  })
})
