# 0002. Exposure through `tailscale serve`

- **Status:** accepted
- **Date:** 2026-11-23
- **Supersedes:** the exposure bullet of [ADR-0001](0001-architecture-and-stack.md)

## Context

ADR-0001 planned to bind the server to the Tailscale address and terminate TLS
in-process with a certificate from `tailscale cert`. The machine already runs
Tailscale, and its `tailscale serve` daemon owns HTTPS on the tailnet and
renews the certificate itself. Binding the Tailscale address as well would
duplicate certificate management and widen the listen surface.

## Decision

- Dandelion listens on `127.0.0.1:7443` over plain HTTP and nothing else.
  Non-loopback bind addresses are still refused.
- `tailscale serve` terminates TLS for the tailnet and proxies to
  `127.0.0.1:7443`. `just tailnet-serve` runs `tailscale serve --bg 7443`;
  the operator owns this command. Dandelion never changes host networking and
  never uses `tailscale funnel`.
- The public origin is `DANDELION_PUBLIC_URL` when set, otherwise
  `https://<tailscale status --json Self.DNSName>` with the trailing dot
  stripped. Startup fails when neither source yields an origin. The repository
  holds no tailnet name or URL.
- A localhost HTTP origin (`http://localhost[:port]`) is accepted only as an
  explicit `DANDELION_PUBLIC_URL` for local development. WebAuthn relying
  parties must be domain names, so `127.0.0.1` and `[::1]` origins are
  refused at startup. Any other plain-HTTP origin is refused.
- The session cookie is `__Host-dandelion_session` (HttpOnly, Secure,
  SameSite=Lax) on an HTTPS origin. On the loopback development origin it is
  `dandelion_session` without `Secure`, which browsers would otherwise drop
  over plain HTTP; only that origin's name is read, so a plain cookie cannot
  shadow the `__Host-` session.

## Consequences

- One owner of TLS and certificate renewal: tailscaled.
- The process has no listening socket outside loopback, so a
  `tailscale serve` misconfiguration cannot expose a plain-HTTP API.
- Local development uses the Vite dev server origin (for example
  `http://localhost:5173`) or the loopback origin directly.
- WebAuthn needs a stable origin; changing the tailnet name or the local port
  invalidates existing passkeys, so the CLI reset path stays available.
