// Applies the theme before first paint: a saved choice wins, otherwise the OS
// colour scheme decides. It is an external file because the Content-Security-Policy
// allows only same-origin scripts; web/src/app/theme.ts owns the same storage key
// and classes and repeats this resolution once the app mounts.
;(() => {
  let saved = null
  try {
    const stored = localStorage.getItem('dandelion-theme')
    if (stored === 'light' || stored === 'dark') saved = stored
  } catch {
    // Storage can be blocked; the OS preference still decides below.
  }
  const systemDark = window.matchMedia?.('(prefers-color-scheme: dark)').matches ?? false
  const theme = saved ?? (systemDark ? 'dark' : 'light')
  document.documentElement.classList.toggle('dark', theme === 'dark')
  document.documentElement.dataset.theme = theme
  document.documentElement.style.colorScheme = theme
  const meta = document.querySelector('meta[name="theme-color"]')
  if (meta) meta.setAttribute('content', theme === 'dark' ? '#0c0e11' : '#f4f4f2')
})()
