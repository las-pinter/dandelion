import { useSyncExternalStore } from 'react'
import {
  DARK_QUERY,
  applyTheme,
  currentTheme,
  readStoredTheme,
  resolveTheme,
  storeTheme,
  systemTheme,
  type Theme,
} from './theme'

/**
 * The theme on `<html>`, held once for the whole app. A saved choice wins;
 * otherwise the OS decides and keeps deciding while the page stays open.
 * `/theme-init.js` applies the same resolution before the first paint.
 */
let theme: Theme = currentTheme()
// A chosen theme holds even when storage refuses the write, so a later OS
// change must not take it back from the owner.
let chosen = readStoredTheme() !== null
const listeners = new Set<() => void>()

function emit(): void {
  for (const listener of listeners) listener()
}

function set(next: Theme): void {
  if (theme === next) return
  theme = next
  applyTheme(next)
  emit()
}

/** Applies and saves an explicit choice, which then wins over the OS. */
export function chooseTheme(next: Theme): void {
  chosen = true
  storeTheme(next)
  set(next)
}

/** Follows OS changes while no choice is saved; returns the unsubscribe. */
export function followSystemTheme(): () => void {
  const media = window.matchMedia?.(DARK_QUERY)
  if (!media) return () => {}
  const follow = () => {
    if (chosen) return
    set(resolveTheme(null, systemTheme()))
  }
  media.addEventListener('change', follow)
  return () => media.removeEventListener('change', follow)
}

/** Re-reads `<html>` and the saved value; for tests and the Storybook toolbar. */
export function resetThemeStore(): void {
  chosen = readStoredTheme() !== null
  theme = currentTheme()
  emit()
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}

function snapshot(): Theme {
  return theme
}

/** The theme currently rendered, re-read whenever it changes. */
export function useTheme(): Theme {
  return useSyncExternalStore(subscribe, snapshot)
}
