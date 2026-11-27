# Document collection manifests

Verified against the implementation and its tests on 2026-11-27. The crate is
`crates/documents`; the server routes are in `crates/server/src/documents.rs`.
Decision record: [ADR-0007](../adr/0007-document-collections.md).

## 1. Where manifests live

One JSON file per collection at `~/.config/dandelion/plugins/<id>.json`.

- The folder is `$DANDELION_PLUGINS_DIR` when set, otherwise
  `$XDG_CONFIG_HOME/dandelion/plugins`, otherwise
  `$HOME/.config/dandelion/plugins`.
- Only `*.json` files are read, at most 64, sorted by file name. A missing
  folder is an empty page, not an error.
- The file stem must equal the manifest's `id`.

## 2. The manifest

```json
{
  "schema": 1,
  "id": "probe-visuals",
  "name": "Probe visuals",
  "icon": "chart",
  "root": "~/agent-notes/project-notes",
  "include": ["{project}/visuals/{day}/**/*.{html,md}"],
  "group": ["project", "day"]
}
```

| Field | Rule |
|-------|------|
| `schema` | Must be `1`. Any other value is a warning row. |
| `id` | `[a-z0-9]` then up to 63 of `[a-z0-9-]`; must equal the file stem. |
| `name` | 1–80 characters after trimming; shown as the collection heading. |
| `icon` | Optional; one of `document`, `chart`, `slides`, `image`, `code`. Anything else falls back to `document`. |
| `root` | `~`, `~/…`, or an absolute path. It must resolve (after canonicalisation) inside the owner's home. `..` is refused. |
| `include` | 1–16 patterns, relative to the root. |
| `group` | Optional; at most 3 capture names, in display order, each starting with a letter and appearing in some pattern. Omitted means one ungrouped list. Unknown top-level keys are ignored. |

A file that fails any rule is skipped and reported as
`{ "file": "<name>.json", "problem": "<the first rule it broke>" }` in the
snapshot's `warnings`. It never fails the page.

## 3. Patterns

- A pattern is split on `/`. Each segment is one of:
  - `**` — any number of path segments, including none;
  - `{name}` covering a whole segment — captures that segment's text under
    `name` (only when `name` is in `group`);
  - anything else — a literal form where `*` matches any run of characters
    inside the segment, and `{a,b}` alternates (for example `*.{html,md}`).
- A brace group without a comma is a malformed capture, so it is refused.
- A path matches when every segment matches and nothing is left over.
  `..`, absolute paths, empty segments, and trailing slashes never match.

## 4. The listing

`GET /api/documents` answers:

```json
{
  "collections": [{
    "id": "probe-visuals",
    "name": "Probe visuals",
    "icon": "chart",
    "root": "~/agent-notes/project-notes",
    "groups": [{
      "values": ["dandelion", "2026-11-27"],
      "documents": [{
        "path": "dandelion/visuals/2026-11-27/1611-first-release.html",
        "title": "First release",
        "kind": "html",
        "size": 41000,
        "modified_ms": 1795344000000
      }]
    }]
  }],
  "warnings": []
}
```

- The walk starts at the canonical root, skips symlinks, stops at depth 12,
  and keeps the first 2,000 files of at most 8 MiB each.
- A file is listed when its extension is `.html`/`.htm` (kind `html`) or
  `.md`/`.markdown` (kind `markdown`) — case-insensitive — and it matches one
  include pattern.
- `values` are the capture values in `group` order. Groups are ordered by
  their newest document, then by values; documents inside a group are newest
  first, then by path. Collections are ordered by name.
- `title` comes from the file name: `-` and `_` become spaces and the first
  letter is uppercased. A leading four-digit part is dropped unless the part
  after it is exactly two digits, so a writer's time prefix goes
  (`1611-first-release.html` → `First release`) and a date prefix stays
  (`2026-11-27-notes.md` → `2026 11 27 notes`).

## 5. The file route

`GET /api/documents/{id}/file?path=<relative path>` serves one document when
the id names a valid manifest and the path is one the listing would produce.

| Check | Answer on failure |
|-------|-------------------|
| Manifest re-read and valid | 404 |
| Path relative, no `..`, `.`, or empty segment | 404 |
| Matches an include pattern; known extension | 404 |
| No component on the way is a symlink | 404 |
| Canonical path still under the root | 404 |
| File at most 8 MiB | 413 |
| Read succeeds | 200 |

Headers on a 200:

| Kind | Content-Type | Content-Security-Policy |
|------|--------------|-------------------------|
| `html` | `text/html; charset=utf-8` | `sandbox allow-scripts; default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'; img-src data:; font-src data:; connect-src 'none'; frame-ancestors 'self'` |
| `markdown` | `text/plain; charset=utf-8` | none |

Both carry `X-Content-Type-Options: nosniff`, `Referrer-Policy:
no-referrer`, and `Cache-Control: no-store`. The page frames HTML with
`sandbox="allow-scripts"`; the CSP's `sandbox` directive puts the document in
an opaque origin even when opened directly, so its scripts have no origin, no
API, and no cookie.

Markdown is rendered by the app with raw HTML disabled. An image is drawn only
when its source is an inline `data:image/…` URI; any other image shows its alt
text instead, so a document cannot make the app origin fetch a remote beacon.

## 6. Writing a manifest

A tool that wants a collection writes one file and needs no Dandelion change:

```sh
mkdir -p "$HOME/.config/dandelion/plugins"
cat > "$HOME/.config/dandelion/plugins/my-visuals.json" <<'JSON'
{
  "schema": 1,
  "id": "my-visuals",
  "name": "My visuals",
  "root": "~/documents",
  "include": ["{project}/**/*.{html,md}"],
  "group": ["project"]
}
JSON
```

A relative `root` is refused; use `~` or an absolute path under the home.
A missing root or a folder outside the home makes the manifest a warning row.
