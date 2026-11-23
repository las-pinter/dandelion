# Dandelion — agent guide

Dandelion is one Rust binary that serves a React web app for monitoring and
controlling this Linux development machine: system health, processes,
services, containers, terminals, and AI agents (including herdr). Treat it as
a remote root-capable shell: every change near auth, input handling, process
spawning, or sudo is **security-sensitive** and says so in its PR.

## Gate

`just ci` is the gate: format check, typecheck, lint, tests, build. CI runs
exactly the same recipe. Run `just setup` once per clone or worktree. `just`
lists every recipe. `gen-check` regenerates the API client from an empty
directory and fails on any working-tree change, so commit the generated files
before running the gate.

## Parallel work

- One task, one branch, one git worktree: `git worktree add ../dandelion-<slug> -b <type>/<slug>`.
- Commits follow Conventional Commits (`feat(agents): …`); the commit-msg hook enforces it. Scopes are crate or page names.
- `main` is always green. Changes land as PRs, squash-merged after CI.
- The layout below removes shared hotspot files. Keep it that way: when a change needs a central list, add a registration seam instead.

## Layout

- `crates/<domain>/` — one Rust crate per domain, package `dandelion-<domain>`. `server` owns HTTP, WebSocket, auth, and wiring; domain crates own their logic and expose plain Rust APIs. `core` holds types shared by several crates.
- `web/src/components/<Name>/` — one component per folder: `<Name>.tsx`, `<Name>.test.tsx`, `<Name>.stories.tsx` (Storybook, `just storybook`), and its helpers. `components/ui/` holds generated shadcn primitives (add with `pnpm dlx shadcn add`); app code imports `cn` from `@/lib/utils`.
- `web/src/pages/<screen>/page.tsx` — a thin screen that composes components and exports `page: PageDefinition`. The page registry (`web/src/app/page-registry.ts`) discovers it: routes and navigation never need a manual edit.
- `web/src/api/gen/` — generated from the server's OpenAPI. Regenerate it, never edit it.
- `web/src/lib/`, `web/src/hooks/` — logic shared by several components.
- `docs/adr/` — architecture decisions. A change that alters a decision there adds a new ADR that supersedes it.
- `docs/tech/` — verified facts about external systems (herdr API, Claude Code state).

## Code rules

- Rust: workspace lints (clippy pedantic, `unsafe` forbidden) apply to every crate via `[lints] workspace = true`. Errors are `thiserror` types in libraries, `anyhow` only in the binary.
- Web: strict TypeScript, no `any`. Style through Tailwind and the design tokens in `web/src/styles/tokens.css`; follow `docs/design.md`.
- Files stay under 500 lines; split by responsibility before that.
- Every mutating API action writes an audit log entry.
- Tests sit next to the code: Rust `#[cfg(test)]` modules or `crates/<domain>/tests/`, web `*.test.ts(x)` beside the source.
