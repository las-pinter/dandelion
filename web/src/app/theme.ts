/**
 * Theme state. A saved choice wins; without one the theme follows the OS and
 * keeps following it while the page is open. The choice is persisted in
 * localStorage and applied to <html> before first paint by /theme-init.js
 * (served from web/public), because the CSP admits only same-origin scripts.
 */
export type Theme = 'dark' | 'light'

export const THEME_STORAGE_KEY = 'dandelion-theme'

/** The OS colour scheme media query. */
export const DARK_QUERY = '(prefers-color-scheme: dark)'

export function isTheme(value: unknown): value is Theme {
  return value === 'dark' || value === 'light'
}

/** Read the saved theme, or `null` when the owner has not chosen one. */
export function readStoredTheme(storage?: Pick<Storage, 'getItem'>): Theme | null {
  try {
    const target = storage ?? window.localStorage
    const value = target.getItem(THEME_STORAGE_KEY)
    return isTheme(value) ? value : null
  } catch {
    return null
  }
}

/** The theme the OS asks for; light when the browser cannot say. */
export function systemTheme(
  matches: (query: string) => boolean = (query) => window.matchMedia?.(query).matches ?? false,
): Theme {
  return matches(DARK_QUERY) ? 'dark' : 'light'
}

/** The theme to render: the saved choice, else the OS preference. */
export function resolveTheme(stored: Theme | null, system: Theme): Theme {
  return stored ?? system
}

/** Save the theme; storage can be blocked, so a failure is not an error. */
export function storeTheme(theme: Theme, storage?: Pick<Storage, 'setItem'>): void {
  try {
    const target = storage ?? window.localStorage
    target.setItem(THEME_STORAGE_KEY, theme)
  } catch {
    // Private browsing and blocked storage still let the theme apply for this visit.
  }
}

/** Keep browser chrome in step with the background token. */
function syncThemeColor(root: HTMLElement): void {
  const meta = document.querySelector('meta[name="theme-color"]')
  if (!meta) return
  const background =
    getComputedStyle(root).getPropertyValue('--background').trim() ||
    root.style.getPropertyValue('--background').trim()
  if (background) meta.setAttribute('content', background)
}

/** Apply the theme classes the tokens and Tailwind use. */
export function applyTheme(theme: Theme, root: HTMLElement = document.documentElement): void {
  root.classList.toggle('dark', theme === 'dark')
  root.dataset.theme = theme
  root.style.colorScheme = theme
  syncThemeColor(root)
}

/** The theme currently rendered on <html>. */
export function currentTheme(root: HTMLElement = document.documentElement): Theme {
  return root.classList.contains('dark') ? 'dark' : 'light'
}
