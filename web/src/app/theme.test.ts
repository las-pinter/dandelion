import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import {
  applyTheme,
  currentTheme,
  DARK_QUERY,
  isTheme,
  readStoredTheme,
  resolveTheme,
  storeTheme,
  systemTheme,
  THEME_STORAGE_KEY,
} from './theme'

function storage(getItem: Storage['getItem']): Pick<Storage, 'getItem'> {
  return { getItem }
}

afterEach(() => {
  document.documentElement.classList.remove('dark', 'light')
  delete document.documentElement.dataset.theme
  document.documentElement.style.colorScheme = ''
})

describe('isTheme', () => {
  it('accepts only the two themes', () => {
    expect(isTheme('dark')).toBe(true)
    expect(isTheme('light')).toBe(true)
    expect(isTheme('sepia')).toBe(false)
    expect(isTheme(null)).toBe(false)
  })
})

describe('readStoredTheme', () => {
  it('is null when nothing is stored, so the OS decides', () => {
    expect(readStoredTheme(storage(() => null))).toBeNull()
  })

  it('reads a valid stored theme', () => {
    expect(readStoredTheme(storage(() => 'light'))).toBe('light')
  })

  it('is null on an invalid value', () => {
    expect(readStoredTheme(storage(() => 'neon'))).toBeNull()
  })

  it('is null when storage throws', () => {
    const throwing = storage(() => {
      throw new DOMException('blocked')
    })

    expect(readStoredTheme(throwing)).toBeNull()
  })

  it('is null when localStorage itself throws', () => {
    const original = Object.getOwnPropertyDescriptor(window, 'localStorage')
    Object.defineProperty(window, 'localStorage', {
      configurable: true,
      get() {
        throw new DOMException('blocked')
      },
    })
    try {
      expect(readStoredTheme()).toBeNull()
    } finally {
      if (original) Object.defineProperty(window, 'localStorage', original)
      else Reflect.deleteProperty(window, 'localStorage')
    }
  })
})

describe('systemTheme and resolveTheme', () => {
  it('reads the OS colour scheme', () => {
    expect(systemTheme(() => true)).toBe('dark')
    expect(systemTheme(() => false)).toBe('light')
  })

  it('prefers a saved choice over the OS', () => {
    expect(resolveTheme('light', 'dark')).toBe('light')
    expect(resolveTheme('dark', 'light')).toBe('dark')
    expect(resolveTheme(null, 'dark')).toBe('dark')
    expect(resolveTheme(null, 'light')).toBe('light')
  })
})

describe('storeTheme', () => {
  it('writes under the shared key', () => {
    const setItem = vi.fn<(key: string, value: string) => void>()

    storeTheme('light', { setItem })

    expect(setItem).toHaveBeenCalledWith(THEME_STORAGE_KEY, 'light')
  })

  it('does not throw when storage is blocked', () => {
    const setItem = () => {
      throw new DOMException('blocked')
    }

    expect(() => storeTheme('light', { setItem })).not.toThrow()
  })

  it('does not throw when localStorage itself throws', () => {
    const original = Object.getOwnPropertyDescriptor(window, 'localStorage')
    Object.defineProperty(window, 'localStorage', {
      configurable: true,
      get() {
        throw new DOMException('blocked')
      },
    })
    try {
      expect(() => storeTheme('light')).not.toThrow()
    } finally {
      if (original) Object.defineProperty(window, 'localStorage', original)
      else Reflect.deleteProperty(window, 'localStorage')
    }
  })
})

describe('applyTheme', () => {
  it('toggles the dark class, dataset, and color scheme', () => {
    applyTheme('light')
    expect(document.documentElement).not.toHaveClass('dark')
    expect(document.documentElement.dataset.theme).toBe('light')
    expect(document.documentElement.style.colorScheme).toBe('light')

    applyTheme('dark')
    expect(document.documentElement).toHaveClass('dark')
    expect(document.documentElement.dataset.theme).toBe('dark')
    expect(document.documentElement.style.colorScheme).toBe('dark')
  })

  it('reports the theme currently rendered on the document', () => {
    applyTheme('light')
    expect(currentTheme()).toBe('light')

    applyTheme('dark')
    expect(currentTheme()).toBe('dark')
  })

  it('keeps the theme-color meta in step with the background token', () => {
    const meta = document.createElement('meta')
    meta.setAttribute('name', 'theme-color')
    document.head.append(meta)
    document.documentElement.style.setProperty('--background', '#101010')
    try {
      applyTheme('light')

      expect(meta).toHaveAttribute('content', '#101010')
    } finally {
      meta.remove()
      document.documentElement.style.removeProperty('--background')
    }
  })
})

describe('pre-paint script', () => {
  const script = readFileSync(resolve(process.cwd(), 'public/theme-init.js'), 'utf8')

  it('reads the same key and media query as the app', () => {
    expect(script).toContain(THEME_STORAGE_KEY)
    expect(script).toContain(DARK_QUERY)
  })
})
