# Dandelion

A self-hosted control room for a Linux development machine: live system
monitoring, processes, systemd services, Docker, browser terminals, and the
AI coding agents running on the machine (with first-class herdr support).

Dandelion runs as a single binary on the machine it controls and is reachable
only over localhost and the Tailscale network.

## Develop

Requirements: Rust (version pinned in `rust-toolchain.toml`), Node 22 with
corepack (pnpm), [`just`](https://github.com/casey/just), `cargo-nextest`,
`cargo-deny`.

```sh
just setup   # install web dependencies and git hooks
just ci      # the full gate: format, typecheck, lint, test, build, e2e
just e2e     # Playwright smoke suite against a scratch server on port 7444
just gen     # regenerate the OpenAPI document and the TypeScript client
just         # list every command
```

The end-to-end suite drives Chromium against a real server and a scratch data
directory under `target/`: it completes the setup ceremony with a browser
virtual authenticator, checks the dashboard's live stream, and signs in with
the password. A fresh Linux needs the browser's system libraries once:
`pnpm --dir web exec playwright install --with-deps chromium`.

Releases are built by CI from a `v*` tag: the workflow runs the full gate,
then attaches the binary and its checksum to the GitHub release. A tag carries
its notes in `docs/release-notes/<tag>.md`; a tag without that file fails the
publish job rather than shipping generated filler.

The server listens on `http://127.0.0.1:7443` and serves `GET /api/health`.
`just dev-web` proxies `/api` there. Non-loopback bind addresses are refused.
The app registers a lifecycle-only service worker, so it can be installed to
a home screen; nothing is cached and no request is intercepted.

## Install and operations

`just install` builds the release binary and installs it as a systemd **user**
service: the binary goes to `~/.local/bin/dandelion`, the units to
`~/.config/systemd/user/`, and the service is enabled and started at once.

```sh
just install    # build, install, enable, start, health check
just deploy     # upgrade: rebuild, back up, replace the binary, restart
just backup     # write a database backup now
just uninstall  # remove the units and the binary; keeps data and settings
```

- **Settings:** the unit reads `~/.config/dandelion/env` when it exists
  (created on first install with commented examples). It may set
  `DANDELION_PUBLIC_URL`, `DANDELION_DATA_DIR`, or `RUST_LOG`. Keep it to
  plain `KEY=value` lines with comments: systemd parses it, and the recipes
  source it so they see the same settings. If your shell exports
  `XDG_DATA_HOME`, set `DANDELION_DATA_DIR` in the env file too: the service
  does not inherit your shell's environment.
- **Boot:** the service is a user unit, so it needs lingering to start without
  a login. `just install` warns when `loginctl show-user $USER -p Linger` is
  not `yes`; enable it once with `sudo loginctl enable-linger $USER`. Logs go
  to the user journal: `journalctl --user -u dandelion`.
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
- **Sudoers:** `just install-sudoers` is separate because it needs a password;
  see [Services and sudo](#services-and-sudo).

## Authentication and exposure

Dandelion is single-owner: one passkey plus an argon2id password, both set
through a single-use setup link, and a 30-day sliding session in an
HttpOnly `__Host-` cookie. The public routes are `/api/health`,
`/api/openapi.json`, and the pre-session auth endpoints (`/api/auth/status`,
setup, and login). Everything else, including `/api/auth/me` and logout,
needs a session.

- **First run or reset:** `dandelion serve` prints a setup link when no owner
  exists; with the installed service, read it from
  `journalctl --user -u dandelion`. `dandelion auth reset` wipes every passkey, password, and session,
  then prints a fresh link valid for 15 minutes. Add `--data-dir` if the
  database is not in the default location.
- **Public origin:** set `DANDELION_PUBLIC_URL` (for example
  `https://host.example.ts.net`) or let the server read `Self.DNSName` from
  `tailscale status --json`. Startup fails when neither yields an origin.
  Plain HTTP is accepted only for `http://localhost[:port]`: WebAuthn relying
  parties are domain names, so `127.0.0.1` and `[::1]` origins cannot host
  passkeys and are refused at startup.
- **Tailnet exposure:** `just tailnet-serve` runs
  `sudo tailscale serve --bg 7443`. Writing serve configuration needs root,
  so a password prompt is expected; alternatively run `just tailnet-operator`
  once to allow the current user. Tailscale terminates TLS and proxies to
  loopback; the app never binds a non-loopback address or changes host
  networking.
- **Start the server:** `just install` does this as a user service. To run it
  by hand instead (no service installed), use `cargo run --release -p
  dandelion-server -- serve` or `./target/release/dandelion serve`; it prints
  the setup link on the first start. Both cannot share port 7443, so stop the
  service first (`systemctl --user stop dandelion`).
- **Local development:** run the server with
  `DANDELION_PUBLIC_URL=http://localhost:5173`, then use
  `http://localhost:5173` so the WebAuthn relying party matches the page.
  On a loopback origin the session cookie is named `dandelion_session`
  without `Secure`, because browsers refuse a `Secure` cookie over plain
  HTTP; an HTTPS origin always uses `__Host-dandelion_session`.

Every failed login is rate-limited after five attempts in fifteen minutes and
recorded in the audit log.

The SQLite database path is chosen in this order: `--data-dir`,
`DANDELION_DATA_DIR`, `$XDG_DATA_HOME/dandelion`, then
`~/.local/share/dandelion`. The directory this process creates is private
(`0700`) and the database file is `0600`. Embedded migrations run on start, and
control actions append to the `audit_log` table.

## Services and sudo

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
allowlist, and restart the server so it reads the new rules. `just
install-sudoers --data-dir /path` reads a non-default data directory.
`dandelion sudoers` prints the drop-in without installing it. Without the
drop-in, system-unit actions report that passwordless sudo is unavailable, and
the refusal is audited.

## Terminal

The Terminal page opens a shell as the server account, one per terminal tab,
rendered with xterm.js (loaded only when that page needs it). The shell runs on
a real pseudo-terminal: raw bytes ride a dedicated WebSocket in binary frames,
and resizes arrive as small JSON control frames. A shell lives only as long as
its socket, so closing the tab, the browser, or the session ends it; opening
and closing are audited, the typed input never is. At most eight shells may be
open at once.

## Agents

The Agents page lists live agents from herdr-core (classic herdr is the
fallback), shows each one's state and blocked reason, streams its pane over a
dedicated socket, and sends a prompt, approves or denies a waiting prompt,
interrupts, kills, or starts a new agent in a working directory. A selected
agent is one chat surface: the transcript as history, the live terminal strip
where the turn happens, and the prompt at the bottom. The strip opens when a
prompt is sent, stays while the agent works, is blocked, or has failed, and
collapses once it is idle or done; a toggle shows or hides it at any time. The
strip draws the agent's own terminal size, scaling the text to fit the strip's
width (never smaller than a readable size; wider terminals keep their right
columns outside the view) and keeps the live bottom row in view, so its
cursor-addressed output is never scrambled. The pane is read-only: viewing
must not resize the agent's real terminal, so control goes through the audited
action API. State changes arrive on the `agents` WebSocket topic. If no
terminal backend answers at startup, a background retry adopts one when it
appears, so a reboot needs no manual restart. A session with a discoverable
transcript renders it in the chat: rendered markdown, collapsed thinking and
tool calls, and nested subagents, streamed
read-only from Claude Code's file and never stored. When a session's Claude
Code state directory is readable, the detail
panel also shows the session id, model, token totals, estimated cost, and what
a waiting session waits for; those totals come from tailing the transcript for
accounting fields only. A Claude session started outside herdr (a plain shell)
still appears in the list, marked unmanaged, without controls; selecting one
shows its transcript and nothing else.
herdr-core has no approve call, so approval presses Enter and denial presses
Escape. When the agent is blocked on a question, the prompt box gives way to an
answer bar: Up and Down move the highlight, Accept sends Enter, Skip sends
Escape, and a typed answer is forced into the dialog through the `answer`
action. That action is audited with its target only; the typed answer is never
stored, like every other typed input. Starting an agent means running a
command line in a directory, usually a worktree path; herdr-core has no
workspace object of its own.

Contributor and agent conventions: [`AGENTS.md`](AGENTS.md).
Architecture decisions: [`docs/adr/`](docs/adr/).

## License

[MIT](LICENSE)
