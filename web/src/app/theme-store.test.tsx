import { act, render, screen } from '@testing-library/react'
import { THEME_STORAGE_KEY } from './theme'
import { chooseTheme, followSystemTheme, resetThemeStore, useTheme } from './theme-store'

function Probe() {
  const theme = useTheme()
  return <span data-testid="theme">{theme}</span>
}

interface MediaStub {
  fire: (matches: boolean) => void
}

/** jsdom has no `matchMedia`; this stub reports the OS scheme and fires changes. */
function stubMedia(dark: boolean): MediaStub {
  const listeners = new Set<() => void>()
  const media = {
    matches: dark,
    addEventListener: (_type: string, listener: () => void) => listeners.add(listener),
    removeEventListener: (_type: string, listener: () => void) => listeners.delete(listener),
  }
  vi.stubGlobal('matchMedia', () => media)
  return {
    fire: (matches) => {
      media.matches = matches
      act(() => {
        for (const listener of [...listeners]) listener()
      })
    },
  }
}

beforeEach(() => {
  vi.restoreAllMocks()
  localStorage.clear()
  document.documentElement.classList.remove('dark', 'light')
  resetThemeStore()
})

afterEach(() => {
  vi.unstubAllGlobals()
  document.documentElement.classList.remove('dark', 'light')
  delete document.documentElement.dataset.theme
  document.documentElement.style.colorScheme = ''
})

describe('theme store', () => {
  it('follows an OS change while no choice is saved', () => {
    const media = stubMedia(false)
    const stop = followSystemTheme()
    render(<Probe />)

    media.fire(true)

    expect(screen.getByTestId('theme')).toHaveTextContent('dark')
    expect(document.documentElement).toHaveClass('dark')
    expect(localStorage.getItem(THEME_STORAGE_KEY)).toBeNull()
    stop()
  })

  it('lets a saved choice win over the OS', () => {
    localStorage.setItem(THEME_STORAGE_KEY, 'light')
    resetThemeStore()
    const media = stubMedia(true)
    const stop = followSystemTheme()
    render(<Probe />)

    media.fire(true)

    expect(screen.getByTestId('theme')).toHaveTextContent('light')
    expect(document.documentElement).not.toHaveClass('dark')
    stop()
  })

  it('holds a chosen theme when storage refuses the write, even across an OS change', () => {
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new DOMException('blocked')
    })
    const media = stubMedia(false)
    const stop = followSystemTheme()
    render(<Probe />)

    act(() => chooseTheme('dark'))

    expect(document.documentElement).toHaveClass('dark')
    expect(localStorage.getItem(THEME_STORAGE_KEY)).toBeNull()

    media.fire(false)

    expect(screen.getByTestId('theme')).toHaveTextContent('dark')
    expect(document.documentElement).toHaveClass('dark')
    stop()
  })
})
