# Dandelion

[![CI](https://github.com/las-pinter/dandelion/actions/workflows/ci.yml/badge.svg)](https://github.com/las-pinter/dandelion/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A self-hosted control room for a Linux development machine. One Rust binary
serves a React app that monitors and controls the machine it runs on: system
health, processes, systemd services, Docker containers, browser terminals,
and the AI coding agents working on it.

Dandelion runs as a systemd user service on the machine it controls. It
listens on loopback only; Tailscale reaches it through `tailscale serve`, so
no port is exposed beyond the machine. Login is a passkey plus a password,
and every control action lands in an audit log.

**Contents**: [Pages](#pages) · [Install](#install) · [Configuration](#configuration) ·
[Authentication](#authentication) · [Exposure](#exposure) ·
[Operations](#operations) · [Development](#development) ·
[Documentation](#documentation)

## Pages

| Page | What it does |
|------|--------------|
| **Dashboard** | Live CPU, memory, disk, and network, with 1 h and 7 d history charts, the host card, and the live agent list. |
| **Processes** | Every readable process, searchable, with signals and nice values. |
| **Services** | User and system systemd units, their journal tail, and unit actions gated by a sudo allowlist. |
| **Containers** | Docker containers with start, stop, restart, and the log tail. |
| **Terminal** | Real pseudo-terminals in the browser, one per tab. |
| **Agents** | Live agents from herdr-core or herdr, with a chat view, prompts, answers, approve/deny, and spawn. |
| **Documents** | HTML and Markdown documents that other tools on the machine declare with a manifest. |

### Services and sudo

The Services page lists user and system units and reads a unit's journal tail.
User-unit actions run over D-Bus as this account. A system-unit action needs an
entry in `<data-dir>/sudo-allowlist.toml`:

```toml
[[rules]]
unit = "nginx.service"
verbs = ["restart", "start"]
```

`just install-sudoers` renders that file into
`/etc/sudoers.d/dandelion-services` (`0440`, root) and validates it with
`visudo -c`; a password prompt is expected once. Rerun it after changing the
allowlist, and restart the server so it reads the new rules.
`just install-sudoers --data-dir /path` reads a non-default data directory.
`dandelion sudoers` prints the drop-in without installing it. Without the
drop-in, system-unit actions report that passwordless sudo is unavailable, and
the refusal is audited.

### Terminal

The Terminal page opens a shell as the server account, one per terminal tab,
rendered with xterm.js (loaded only when that page needs it). The shell runs on
a real pseudo-terminal: raw bytes ride a dedicated WebSocket in binary frames,
and resizes arrive as small JSON control frames. A shell lives only as long as
its socket, so closing the tab, the browser, or the session ends it; opening
and closing are audited, the typed input never is. At most eight shells may be
open at once.

### Agents

The Agents page lists live agents from herdr-core (classic herdr is the
fallback), shows each one's state and blocked reason, streams its pane over a
dedicated socket, and sends a prompt, approves or denies a waiting prompt,
interrupts, kills, or starts a new agent in a working directory.

A selected agent is one chat surface: the transcript as history, the live
terminal strip where the turn happens, and the prompt at the bottom. The strip
opens when a prompt is sent, stays while the agent works, is blocked, or has
failed, and collapses once it is idle or done; a toggle shows or hides it at
any time, and either pane can fill the chat. The strip draws the agent's own
terminal size, scaling the text to fit the strip's width (never smaller than a
readable size; wider terminals keep their right columns outside the view) and
keeps the live bottom row in view, so its cursor-addressed output is never
scrambled.

- The pane is read-only: viewing must not resize the agent's real terminal, so
  control goes through the audited action API.
- State changes arrive on the `agents` WebSocket topic. If no terminal backend
  answers at startup, a background retry adopts one when it appears, so a
  reboot needs no manual restart.
- A session with a discoverable transcript renders it in the chat: rendered
  Markdown, collapsed thinking and tool calls, and nested subagents, streamed
  read-only from Claude Code's file and never stored.
- When a session's Claude Code state directory is readable, the detail panel
  also shows the session id, model, token totals, estimated cost, and what a
  waiting session waits for; those totals come from tailing the transcript for
  accounting fields only.
- A Claude session started outside herdr (a plain shell) still appears in the
  list, marked unmanaged, without controls; selecting one shows its transcript
  and nothing else.
- herdr-core has no approve call, so approval presses Enter and denial presses
  Escape. When the agent is blocked on a question, the prompt box gives way to
  an answer bar: Up and Down move the highlight, Accept sends Enter, Skip
  sends Escape, and a typed answer is forced into the dialog through the
  `answer` action. That action is audited with its target only; the typed
  answer is never stored, like every other typed input.
- Starting an agent means running a command line in a directory, usually a
  worktree path; herdr-core has no workspace object of its own.

### Documents

The Documents page shows documents that tools on this machine write (HTML
one-pagers, slide decks, Markdown summaries) without Dandelion knowing the
tool. An installed tool declares a collection with a JSON manifest in
`~/.config/dandelion/plugins/<id>.json`; the page lists every valid
collection, grouped by the capture values the manifest names, and a manifest
that breaks a rule becomes a warning row instead of failing the page.
Manifests are data: Dandelion never loads or runs plugin code, and the list
refreshes when the page opens and on a button.

HTML documents are untrusted: the file route answers with a
`Content-Security-Policy: sandbox allow-scripts` that denies network, and the
app frames the document with `sandbox="allow-scripts"`, so its scripts run in
an opaque origin that cannot call the API or read the session cookie.
Markdown is fetched as text and rendered in the app with raw HTML disabled.
Every requested path is resolved against the manifest's root and include
patterns; `..`, absolute paths, unlisted files, and symlinks answer 404, and a
file above 8 MiB answers 413. The format is documented in
[`docs/tech/documents-manifest.md`](docs/tech/documents-manifest.md).

## Install

**Runtime requirements:** Linux with a systemd user session. Docker,
herdr-core or herdr, and Claude Code are optional: the matching page reports
when they are absent and the rest of the app keeps working.

```sh
git clone https://github.com/las-pinter/dandelion && cd dandelion
just setup
just install   # build, install, enable, start, health check
```

`just install` builds the release binary and installs it as a systemd **user**
service: the binary goes to `~/.local/bin/dandelion`, the units to
`~/.config/systemd/user/`, and the service is enabled and started at once.

```sh
just install     # build, install, enable, start, health check
just deploy      # upgrade: rebuild, back up, replace the binary, restart
just backup      # write a database backup now
just uninstall   # remove the units and the binary; keeps data and settings
```

On the first start the server prints a single-use setup link; with the service
installed, read it from the journal:

```sh
journalctl --user -u dandelion
```

The service is a user unit, so it needs lingering to start without a login.
`just install` warns when `loginctl show-user $USER -p Linger` is not `yes`;
enable it once with `sudo loginctl enable-linger $USER`.

The server listens on `http://127.0.0.1:7443` and serves `GET /api/health`.
Non-loopback bind addresses are refused. `just dev-web` runs the Vite dev
server and proxies `/api` to that address.

## Configuration

The unit reads `~/.config/dandelion/env` when it exists (created on first
install with commented examples). Keep it to plain `KEY=value` lines with
comments: systemd parses it, and the recipes source it so they see the same
settings.

| Variable | Meaning |
|----------|---------|
| `DANDELION_PUBLIC_URL` | The origin the app is served from, for WebAuthn and origin checks. Auto-detected from `tailscale status --json` when unset. |
| `DANDELION_DATA_DIR` | The data directory, ahead of the default lookup. |
| `DANDELION_PLUGINS_DIR` | The document-manifest folder; mainly for tests. |
| `RUST_LOG` | The tracing filter; `info` by default. |

If your shell exports `XDG_DATA_HOME`, set `DANDELION_DATA_DIR` in the env
file too: the service does not inherit your shell's environment.

The data directory is chosen in this order: `--data-dir`,
`DANDELION_DATA_DIR`, `$XDG_DATA_HOME/dandelion`, then
`~/.local/share/dandelion`. The directory the server creates is private
(`0700`) and the SQLite database is `0600`; embedded migrations run on start,
and control actions append to the `audit_log` table.

The app registers a lifecycle-only service worker, so it can be installed to a
home screen; nothing is cached and no request is intercepted.

## Authentication

Dandelion is single-owner: one passkey plus an argon2id password, both set
through a single-use setup link, and a 30-day sliding session in an HttpOnly
`__Host-` cookie. The public routes are `/api/health`, `/api/openapi.json`,
and the pre-session auth endpoints (`/api/auth/status`, setup, and login).
Everything else, including `/api/auth/me` and logout, needs a session. Every
failed login is rate-limited after five attempts in fifteen minutes and
recorded in the audit log.

- **First run or reset:** `dandelion serve` prints a setup link when no owner
  exists; with the installed service, read it from
  `journalctl --user -u dandelion`. `dandelion auth reset` wipes every passkey,
  password, and session, then prints a fresh link valid for 15 minutes. Add
  `--data-dir` if the database is not in the default location.
- **Public origin:** set `DANDELION_PUBLIC_URL` (for example
  `https://host.example.ts.net`) or let the server read `Self.DNSName` from
  `tailscale status --json`. Startup fails when neither yields an origin.
  Plain HTTP is accepted only for `http://localhost[:port]`: WebAuthn relying
  parties are domain names, so `127.0.0.1` and `[::1]` origins cannot host
  passkeys and are refused at startup.
- **Local development:** run the server with
  `DANDELION_PUBLIC_URL=http://localhost:5173`, then use
  `http://localhost:5173` so the WebAuthn relying party matches the page. On a
  loopback origin the session cookie is named `dandelion_session` without
  `Secure`, because browsers refuse a `Secure` cookie over plain HTTP; an
  HTTPS origin always uses `__Host-dandelion_session`.
- **By hand:** `cargo run --release -p dandelion-server -- serve` or
  `./target/release/dandelion serve`. Both cannot share port 7443 with the
  service, so stop it first (`systemctl --user stop dandelion`).

### Security posture

Dandelion is a remote root-capable shell, so the rules are strict:

- Loopback only; TLS and tailnet reachability belong to `tailscale serve`.
- A session cookie, an origin check on every mutating request, and an Origin
  check on every WebSocket handshake.
- Root actions only through the sudoers drop-in built from the allowlist.
- Every control action writes exactly one audit row; typed prompts, keys, and
  answers are never stored, and transcript content never leaves its file.
- Documents render in an opaque-origin sandbox; a manifest is data, never
  code.

## Exposure

`just tailnet-serve` runs `sudo tailscale serve --bg 7443`. Writing serve
configuration needs root, so a password prompt is expected; alternatively run
`just tailnet-operator` once to allow the current user. Tailscale terminates
TLS and proxies to loopback; the app never binds a non-loopback address or
changes host networking. `tailscale funnel` is never used.

## Operations

- **Upgrade:** `git pull` on `main`, then `just deploy`. It reinstalls the
  units, writes a pre-restart backup with the usual retention, replaces the
  binary, and restarts; migrations run on start. A failed backup aborts the
  deploy before the restart.
- **Backups:** the `dandelion backup` subcommand copies the live database with
  SQLite `VACUUM INTO` into `<data-dir>/backups/dandelion-<UTC>.db` (`0600` in
  a `0700` directory) and keeps the newest 7. The `dandelion-backup.timer`
  runs it daily at 03:30 local time with a random delay and catches up after a
  missed run. `just backup` reads the same env file as the unit, so it backs
  up the configured database; pass `--data-dir`, `--out-dir`, or `--keep` to
  override. A custom `--out-dir` must already be private (`0700`); it is
  refused rather than chmodded, and concurrent runs take turns on a lock.
- **Restore:** stop the service (`systemctl --user stop dandelion`), move
  `dandelion.db`, `dandelion.db-wal`, and `dandelion.db-shm` out of the data
  directory, copy the chosen backup in as `dandelion.db`, then start the
  service. The WAL sidecars of the old database must not be left behind.
- **Logs:** `journalctl --user -u dandelion`.

## Development

**Toolchain:** Rust (version pinned in `rust-toolchain.toml`), Node 22 with
corepack (pnpm), [`just`](https://github.com/casey/just), `cargo-nextest`, and
`cargo-deny`. A fresh Linux also needs the browser's system libraries once:
`pnpm --dir web exec playwright install --with-deps chromium`.

| Command | What it does |
|---------|--------------|
| `just setup` | One-time setup after cloning (or in a new worktree): web dependencies and git hooks. |
| `just check` | Fast checks: format, types, lint, web tests. Run it after each edit batch. |
| `just ci` | The full gate: everything in `check`, plus Rust tests, build, and e2e. CI runs the same recipe. |
| `just test` | All tests. |
| `just e2e` | Playwright smoke suite against a scratch server on port 7444. |
| `just gen` | Regenerate the OpenAPI document and the TypeScript client. |
| `just storybook` | Browse the components in isolation. |
| `just` | List every recipe. |

`just e2e` drives Chromium against a real server and a scratch data directory
under `target/`: it completes the setup ceremony with a browser virtual
authenticator, checks the dashboard's live stream, signs in with the password,
and proves that a document's script cannot reach the API.

Releases are built by CI from a `v*` tag: the workflow runs the full gate,
then attaches the binary and its checksum to the GitHub release. A tag carries
its notes in `docs/release-notes/<tag>.md`; a tag without that file fails the
publish job rather than shipping generated filler.

## Documentation

- [`AGENTS.md`](AGENTS.md): contributor and agent conventions, layout, and the
  gate.
- [`docs/adr/`](docs/adr/): architecture decisions.
- [`docs/tech/`](docs/tech/): verified facts about the external systems
  Dandelion talks to.
- [`docs/release-notes/`](docs/release-notes/): one file per release tag.

## License

[MIT](LICENSE)
