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
just ci      # the full gate: format, typecheck, lint, test, build
just gen     # regenerate the OpenAPI document and the TypeScript client
just         # list every command
```

The server listens on `http://127.0.0.1:7443` and serves `GET /api/health`.
`just dev-web` proxies `/api` there. Non-loopback bind addresses are refused.

## Authentication and exposure

Dandelion is single-owner: one passkey plus an argon2id password, both set
through a single-use setup link, and a 30-day sliding session in an
HttpOnly `__Host-` cookie. The public routes are `/api/health`,
`/api/openapi.json`, and the pre-session auth endpoints (`/api/auth/status`,
setup, and login). Everything else, including `/api/auth/me` and logout,
needs a session.

- **First run or reset:** `dandelion serve` prints a setup link when no owner
  exists. `dandelion auth reset` wipes every passkey, password, and session,
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
- **Start the server:** `cargo run --release -p dandelion-server -- serve`
  (or `./target/release/dandelion serve`). It prints the setup link on the
  first start; keep it running while you register the passkey.
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
interrupts, kills, or starts a new agent in a working directory. The pane is
read-only: viewing must not resize the agent's real terminal, so control goes
through the audited action API. State changes arrive on the `agents` WebSocket
topic. When a session's Claude Code state directory is readable, the detail
panel also shows the session id, model, token totals, estimated cost, and what
a waiting session waits for; those totals come from tailing the transcript for
accounting fields only. herdr-core has no approve call, so approval presses
Enter and denial presses Escape. Starting an agent means
running a command line in a directory, usually a worktree path; herdr-core has
no workspace object of its own.

Contributor and agent conventions: [`AGENTS.md`](AGENTS.md).
Architecture decisions: [`docs/adr/`](docs/adr/).

## License

[MIT](LICENSE)
