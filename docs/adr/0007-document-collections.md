# 0007. Documents from declarative plugin manifests

- **Status:** accepted
- **Date:** 2026-11-27

## Context

Tools on this machine write documents the owner wants to read from a phone or
another device: HTML one-pagers and slide decks, and Markdown summaries. The
first writer is the companion-agents `/visualize` command, but Dandelion must
not hardcode that tool or its folders.

Dandelion already runs behind the owner's passkey login over Tailscale, so it
is the natural reader. The documents are written by agents and are untrusted:
they may contain scripts, and they must never share an origin with the app,
which holds the session cookie and a root-capable API.

## Decision

An installed tool declares a collection with a JSON manifest at
`~/.config/dandelion/plugins/<id>.json` (`schema: 1`). Manifests are data:
Dandelion never loads or runs plugin code, and a tool gains a page by writing
one file.

- The manifest names the collection, a root folder under the owner's home, the
  include patterns, and the grouping order. `{name}` in a pattern captures one
  path segment when `name` appears in `group`; any other brace group is
  alternation. `**` crosses directories.
- A manifest that fails validation is skipped and shown as a warning row on
  the page; no manifest can stop the page from loading. Files above 64 KiB and
  patterns beyond 512 bytes, 32 segments, 4 `**` segments, or 256 expanded
  alternatives are refused the same way, so a hostile manifest costs a warning
  row rather than memory or time. The folder can be moved for tests with
  `DANDELION_PLUGINS_DIR`.
- `GET /api/documents` lists collections, groups, and warnings.
  `GET /api/documents/{id}/file?path=…` serves one document the listing would
  produce. Both routes sit behind the session.
- An HTML document answers with
  `Content-Security-Policy: sandbox allow-scripts; default-src 'none'; …;
  connect-src 'none'` (no `allow-same-origin`), plus `nosniff`,
  `no-referrer`, and `no-store`. The page frames it with
  `sandbox="allow-scripts"`. The document runs in an opaque origin: its
  scripts cannot call the API or read the cookie.
- Markdown is served as `text/plain`, fetched by the app, and rendered by the
  bundled `react-markdown` with raw HTML disabled, so nothing in a `.md` file
  executes, and only inline `data:` images are drawn, so it cannot make the
  app origin fetch a remote beacon. Fenced mermaid blocks stay code for now.
- Every request path is resolved against the manifest root and the include
  patterns: `..`, absolute paths, unlisted files, and any symlink are refused
  with 404; files above 8 MiB answer 413. The walk skips symlinks and caps a
  collection at 2,000 documents and depth 12.
- The Documents page is master–detail; the URL carries `?doc=<id>&path=<path>`
  so a document can be bookmarked. The list refreshes on open and on a button;
  there is no watcher and no polling. List and read only: no search, deletion,
  or retention.

## Consequences

- A plugin is a data drop, not a code dependency, so a broken or hostile
  manifest costs a warning row, and documents render without extending the
  app's own CSP or origin.
- Documents live outside the database: nothing is copied, indexed, or audited.
  The files belong to the tools that write them.
- The iframe is still scripted, so a document may be interactive; it simply
  has no network, no storage, and no origin to spend.
- A collection's listing is recomputed per page load and per refresh; a very
  large tree shows its newest 2,000 files.
