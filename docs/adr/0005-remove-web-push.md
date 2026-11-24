# 0005. Remove web push notifications

- **Status:** accepted
- **Date:** 2026-11-24
- **Supersedes:** [ADR-0004](0004-web-push.md)

## Context

ADR-0004 added Web Push so a closed phone could receive alerts. Browser push
always needs a third-party push service: Google FCM on Chromium, Mozilla
autopush on Firefox, or Apple's service on Safari. On a desktop, that service
is often disabled by default (Brave ships with Google push messaging off),
and enabling it makes a third party the transport for every notification the
browser shows, across all sites. There is no self-hosted push service the
browser will accept.

The owner does not want that transport. Live state already travels over the
in-app WebSocket hub, and Dandelion is a tool the owner uses at the desk or
on the phone while the page is open.

## Decision

Remove web push completely:

- delete the `notifications` module, its four routes, the VAPID key handling,
  the trigger state, and the subscription store;
- delete the dashboard Notifications panel, the push hook, and the push-only
  service worker;
- drop the `push_subscriptions` table (migration 0005) and the `web-push`
  dependency, including the advisory ignores it required;
- delete `vapid.json` from the data directory; the server deletes a key left
  by an earlier version when it starts;
- keep live agent, unit, and resource state on the in-app hub only.

## Consequences

- No third-party push service is involved; no notification content or
  metadata leaves the machine.
- A closed or backgrounded browser receives no alerts. The dashboard must be
  open to show state.
- The server loses its native OpenSSL dependency chain and the endpoint SSRF
  surface.
- The push-only worker was the only service worker, so browser PWA
  installability may regress. A lifecycle-only worker can be added later if
  the PWA is wanted; it must cache nothing.
- A browser that had registered the old worker keeps it until its site data is
  cleared; the worker is push-only and no longer registered by the app.
- Removal is forward-only. Migration 0005 drops `push_subscriptions`, and a
  rollback to the push version must restore the feature from git and recreate
  the table by hand, because SQLx will not re-run migration 0004. The key is
  gone too: a restored version generates a new VAPID pair, and every browser
  must subscribe again.
