/* Dandelion's service worker: shows push notifications, caches nothing. */
self.addEventListener('install', () => self.skipWaiting())
self.addEventListener('activate', (event) => event.waitUntil(self.clients.claim()))

self.addEventListener('push', (event) => {
  let message = {}
  try {
    message = event.data ? event.data.json() : {}
  } catch {
    message = {}
  }
  const title = typeof message.title === 'string' ? message.title : 'Dandelion'
  const options = {
    body: typeof message.body === 'string' ? message.body : '',
    tag: typeof message.tag === 'string' ? message.tag : undefined,
    data: { url: typeof message.url === 'string' ? message.url : '/' },
    icon: '/icon-192.png',
    badge: '/icon-192.png',
  }
  event.waitUntil(self.registration.showNotification(title, options))
})

self.addEventListener('notificationclick', (event) => {
  event.notification.close()
  const url =
    event.notification.data && typeof event.notification.data.url === 'string'
      ? event.notification.data.url
      : '/'
  event.waitUntil(
    self.clients.matchAll({ type: 'window', includeUncontrolled: true }).then((windows) => {
      const open = windows.find((client) => 'focus' in client)
      if (open) {
        const focused = open.navigate ? open.navigate(url) : Promise.resolve(open)
        return focused.then(() => open.focus())
      }
      return self.clients.openWindow(url)
    }),
  )
})
