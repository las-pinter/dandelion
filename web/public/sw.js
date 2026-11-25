// Lifecycle only: makes the app installable. Nothing is cached and no request
// is intercepted; the fetch handler is empty on purpose (a registered fetch
// handler is part of the installability criteria).
self.addEventListener('install', () => {
  self.skipWaiting()
})

self.addEventListener('activate', (event) => {
  event.waitUntil(self.clients.claim())
})

self.addEventListener('fetch', () => {})
