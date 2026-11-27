# 0006. A read-only transcript socket

- **Status:** accepted; the bundle consequence is superseded by ADR-0008.
- **Date:** 2026-11-25

## Context

A Claude Code session's transcript is rendered beside the raw agent pane.
The transcript is a growing JSONL file on disk: the browser needs the newest
records on open, appended records while the session runs, and older pages on
demand. It is display-only; Dandelion writes nothing.

Polling REST from the browser would reread the file every few seconds and still
needs a paging cursor. The JSON topic hub (ADR-0001) is topic-tagged, carries
a 64 KiB payload bound, and has no per-client cursor.

## Decision

A dedicated socket per session, `GET /api/claude/{session}/transcript/ws`,
optionally `?agent=<id>` for one subagent file, with the same session-cookie
and origin handshake, 30-second recheck, and logout close as the other
sockets.

- Server frames are JSON text: `page`, `appended`, `earlier`, `subagents`, and
  `error`. The client sends only `{"type":"earlier"}`; the server owns the
  cursor, so a forged offset cannot escape the file.
- A record carries its role, timestamp, and blocks: text, thinking, tool use,
  tool result, and divider. Block field names are camelCase on the wire, like
  every other API field. Text and inputs are bounded at 8 KiB; unknown record
  types are skipped, never fatal.
- The server reads Claude Code's file directly and stores nothing: no SQLite
  row, no audit row, no content cache.
- Session ids go through the existing transcript lookup, and a subagent id
  must be `agent-` followed by letters and digits.

## Consequences

- Three socket shapes exist: JSON topics on `/api/ws`, raw terminal bytes
  (ADR-0003), and JSON transcript records. Each stays purpose-built.
- The tab is best effort. A file that disappears ends the socket with an error
  frame; the raw terminal stays the ground truth.
- Subagent transcripts are separate files, so nesting is one more socket from
  the nested panel. All runs of a session live flat in `<session>/subagents/`
  (a run started by a run included), so every transcript socket sends the
  same list and nesting follows `toolUseId` at any depth.
- Paging scans the file backwards in 64 KiB blocks, so a page costs one block
  plus the page; a record longer than a block is stitched across blocks.
- `react-markdown` and `remark-gfm` load with the agents page (the page
  registry imports every page eagerly), adding roughly 180 kB to that chunk.
  Splitting the renderer out is deferred until the bundle budget matters.
- Earlier pages accumulate in the view with no cap, because the user loads
  each one deliberately; record rows use `content-visibility: auto` and images
  load lazily, which keeps a long history cheap to hold.
- The panel follows the newest record unless the reader scrolled back, and
  loading an earlier page keeps the view where it was.
- When more than 500 records arrive live since the last page, the client
  reopens the socket and shows the newest page: that realigns with the
  server's cursor, and any earlier page loaded by hand is dropped at once.
