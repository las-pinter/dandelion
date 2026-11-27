// Applies the saved theme before first paint. It is an external file because
// the Content-Security-Policy allows only same-origin scripts; web/src/app/theme.ts
// owns the same storage key and classes.
;(() => {
  try {
    const stored = localStorage.getItem('dandelion-theme')
    const theme = stored === 'light' ? 'light' : 'dark'
    document.documentElement.classList.toggle('dark', theme === 'dark')
    document.documentElement.dataset.theme = theme
    document.documentElement.style.colorScheme = theme
    const meta = document.querySelector('meta[name="theme-color"]')
    if (meta) meta.setAttribute('content', theme === 'dark' ? '#07090d' : '#f6f7f9')
  } catch {
    // Storage can be blocked; the dark class in the markup is the fallback.
  }
})()
