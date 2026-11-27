import { render, screen } from '@testing-library/react'
import { MemoryRouter } from 'react-router'
import { describe, expect, it } from 'vitest'
import { NotFound } from './NotFound'

describe('NotFound', () => {
  it('explains the missing page and offers the way back', () => {
    render(
      <MemoryRouter>
        <NotFound />
      </MemoryRouter>,
    )

    expect(screen.getByRole('heading', { name: 'Page not found' })).toBeInTheDocument()
    expect(screen.getByText('Nothing lives at this address.')).toBeInTheDocument()
    expect(screen.getByRole('link', { name: 'Back to the start' })).toHaveAttribute('href', '/')
  })
})
