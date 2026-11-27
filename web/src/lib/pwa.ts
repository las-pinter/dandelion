/**
 * Registers the lifecycle-only service worker so the app can be installed.
 *
 * Registration is best effort: a browser without service workers, or a
 * failed registration, leaves the app fully usable.
 */
export function registerServiceWorker(): void {
  if (!('serviceWorker' in navigator)) return
  navigator.serviceWorker.register('/sw.js').catch((cause: unknown) => {
    console.warn('service worker registration failed', cause)
  })
}
