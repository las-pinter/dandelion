# 0004. Web push for notifications

- **Status:** accepted
- **Date:** 2026-11-24

## Context

Alerts are wanted on the phone when an agent needs a person, a service fails,
or a resource stays high. The app is a PWA served over `tailscale serve`
HTTPS, so the browser push stack is available. A closed phone cannot receive a
message over `/api/ws`; only the push service can wake it.

## Decision

Notifications use the Web Push protocol with VAPID:

- A P-256 key pair lives in `<data-dir>/vapid.json` with `0600` permissions,
  created on first start. The public half is served by
  `GET /api/notifications/key`; the private half signs every message.
- Each browser subscription is one row in `push_subscriptions`. The endpoint
  is a capability URL, so Dandelion's own audit rows, log lines, and errors
  never carry it; the push client's verbose logging is pinned to `warn`
  because it prints the request URI.
- `POST /api/notifications/subscribe`, `/unsubscribe`, and `/test` are
  session-protected and origin-checked like every mutating route; each writes
  one audit row with a fixed target (`push`).
- Delivery uses the `web-push` crate with its hyper client. A push service
  that answers `404` or `410` retires the subscription; other failures are
  logged and counted, and ten consecutive failures retire it too, so a key the
  browser no longer holds is not retried forever. A failure caused by the
  payload or the browser's keys is logged but not counted against the device.
- The service worker (`web/public/sw.js`) only shows notifications and opens
  the matching page on tap. It caches nothing, so the SPA is never stale.
- Triggers, once per transition, are held in memory:
  an agent entering `blocked` (body names the agent and the reason), an agent
  finishing a turn or exiting, a systemd unit entering `failed`, and CPU,
  memory, or a disk staying at or above 90 % for two minutes. A metrics sample
  older than 30 s restarts that two-minute window, so a stopped collector
  cannot age one sample into a sustained-high notice. A Claude Code
  session started outside a herdr terminal is announced when it starts
  waiting; sessions inside one are covered by the backend's status events.
  The owner chose to include the agent name and state in the body. A restart
  may repeat a notice that is still true; it never drops one silently.

## Consequences

- The server grows a native dependency chain (OpenSSL through `ece` and
  `native-tls`); the binary is no longer purely Rust-native.
- `web-push` 0.11 pins `chrono` (unmaintained) and `crypto-bigint` 0.5 (a
  timing advisory in constant-time operations). Both are accepted, with the
  reasoning recorded in `deny.toml`: chrono formats JWT timestamps, and the
  bigint issue is reachable only while signing with Dandelion's own key. The
  ignores are revisited when the crate updates.
- Notifications depend on the browser's push service, not on the tailnet being
  up at that moment; the encrypted payload is stored by the push service until
  delivery or TTL expiry (600 s).
- Conversation content never enters a notification; the payload is a short
  fact plus a route.
- The in-app hub keeps carrying live state; push is a parallel, best-effort
  channel. A test notification reports how many devices accepted it, so a
  browser whose row the server retired no longer looks successful.
- The owner's browser supplies the endpoint, so an authenticated session can
  make the server POST to any public HTTPS host it can reach. The
  single-owner model, the origin check, and the endpoint validation bound
  this: the endpoint must be HTTPS, carry no userinfo, and name no loopback,
  private, link-local, tailnet, or localhost host, including IPv4-mapped IPv6
  literals. A name is resolved once when it is stored and refused if any
  address is not global; an unresolvable name is kept, so DNS rebinding
  remains possible. A public host is still accepted.
- `vapid.json` sits outside SQLite. A backup must include the data directory;
  restoring only the database makes a new key and strands every stored
  subscription. The VAPID `sub` claim is the public origin, or a local mailto
  for a loopback origin.
