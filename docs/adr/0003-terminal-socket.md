# 0003. A dedicated socket for terminals

- **Status:** accepted
- **Date:** 2026-11-23

## Context

ADR-0001 records "one multiplexed WebSocket for live streams with generated
message types". The web terminal (task 14) needs a bidirectional byte channel
per shell: keystrokes out, raw terminal output in, plus a small resize control
message. Routing that through the topic hub would mean base64-encoding every
frame, sizing it against the 64 KiB JSON payload bound, and inventing a topic
per shell, all to avoid a second socket.

## Decision

A terminal gets its own socket: `GET /api/terminals/{id}/ws`, one per shell,
authenticated with the same session cookie and origin check as `/api/ws`.

- Binary frames carry terminal bytes in both directions; text frames carry
  `{"type":"resize","cols":N,"rows":N}`.
- One socket may attach to a terminal at a time; a second gets `409`.
- The socket revalidates its session every 30 seconds and closes on logout.
- The shell ends when the socket does, including the whole process session, so
  a background child cannot hold the pseudo-terminal open.
- Opening and closing a shell are audited; the typed input never is.

The topic hub keeps ADR-0001's role for every other stream. This ADR narrows
its wording; it does not change the hub.

## Consequences

- Two protocols live in the server: JSON topics on `/api/ws` and raw bytes on
  the terminal socket. Each stays small and purpose-built.
- A terminal is root-capable in effect (a shell as the server account). It is
  gated by the owner session and the origin check only, by the owner's
  decision on 2026-11-23.
- xterm.js is loaded on demand, so no other screen pays its bundle cost.
