import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { THEME_STORAGE_KEY } from '@/app/theme'
import { ThemeToggle } from './ThemeToggle'

beforeEach(() => {
  document.documentElement.classList.add('dark')
  localStorage.clear()
})

afterEach(() => {
  document.documentElement.classList.remove('dark', 'light')
  delete document.documentElement.dataset.theme
  document.documentElement.style.colorScheme = ''
  vi.restoreAllMocks()
})

describe('ThemeToggle', () => {
  it('switches to light, applies it, and persists the choice', async () => {
    const user = userEvent.setup()
    render(<ThemeToggle />)

    await user.click(screen.getByRole('button', { name: 'Switch to light theme' }))

    expect(document.documentElement).not.toHaveClass('dark')
    expect(document.documentElement.dataset.theme).toBe('light')
    expect(localStorage.getItem(THEME_STORAGE_KEY)).toBe('light')
    expect(screen.getByRole('button', { name: 'Switch to dark theme' })).toBeInTheDocument()
  })

  it('switches back to dark', async () => {
    const user = userEvent.setup()
    render(<ThemeToggle />)

    await user.click(screen.getByRole('button', { name: 'Switch to light theme' }))
    await user.click(screen.getByRole('button', { name: 'Switch to dark theme' }))

    expect(document.documentElement).toHaveClass('dark')
    expect(document.documentElement.dataset.theme).toBe('dark')
    expect(localStorage.getItem(THEME_STORAGE_KEY)).toBe('dark')
  })

  it('still applies the theme when storage refuses the write', async () => {
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new DOMException('blocked')
    })
    const user = userEvent.setup()
    render(<ThemeToggle />)

    await user.click(screen.getByRole('button', { name: 'Switch to light theme' }))

    expect(document.documentElement).not.toHaveClass('dark')
    expect(screen.getByRole('button', { name: 'Switch to dark theme' })).toBeInTheDocument()
  })
})
