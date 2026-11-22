# 0001. Architecture and stack

- **Status:** accepted
- **Date:** 2026-11-22

## Context

Dandelion monitors and controls the development machine it runs on, including
AI agents managed by herdr. It must be light on resources, safe to expose on a
private network, usable from desktop and phone, and easy for several humans
and AI agents to develop in parallel.

## Decision

- **Server:** Rust (axum, tokio), one binary that embeds the built web app.
- **Web:** React, TypeScript, Vite, Tailwind CSS, shadcn/ui; a static SPA and
  installable PWA, desktop-first with a full phone layout.
- **Contract:** OpenAPI generated from the axum handlers (utoipa), a generated
  TypeScript client, and one multiplexed WebSocket for live streams with
  generated message types. CI fails when generated code drifts.
- **Storage:** embedded SQLite (sqlx, migrations): metric history, sessions,
  audit log.
- **Exposure:** binds only to localhost and the Tailscale address; HTTPS with
  the Tailscale certificate; single-owner login with passkey and password
  fallback.
- **Privilege:** runs as the desktop user via a systemd user service; root
  actions go through a narrow sudoers allowlist.
- **Agents:** herdr's socket API is the primary control path; Claude Code gets
  a dedicated adapter; a process scan finds agents outside herdr.
- **Scope:** one machine; the API keeps a seam for aggregating several later.
- **Repository:** Cargo workspace of domain crates plus `web/` with component
  folders and self-registering pages; `just` as the single command surface;
  trunk-based workflow with a worktree per task.

## Consequences

- One deployable artifact and low idle cost; Rust slows UI-adjacent iteration
  slightly, mitigated by the generated contract.
- The web UI is a root-capable remote shell: auth, origin checks, and the
  audit log are mandatory for every control action.
- herdr and Claude Code are external, versioned dependencies; their adapters
  must detect version changes and degrade to read-only.
