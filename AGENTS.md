# Dandelion — agent guide

Dandelion is one Rust binary that serves a React web app for monitoring and
controlling this Linux development machine: system health, processes,
services, containers, terminals, and AI agents (including herdr). Treat it as
a remote root-capable shell: every change near auth, input handling, process
spawning, or sudo is **security-sensitive** and says so in its PR.

## Gate

`just ci` is the gate: format check, typecheck, lint, tests, build, and the
Playwright smoke suite. CI runs exactly the same recipe. Run `just setup` once
per clone or worktree; a fresh Linux also needs the browser's system
libraries once: `pnpm --dir web exec playwright install --with-deps chromium`.
`just` lists every recipe. `gen-check` regenerates the API client from an empty
directory and fails on any working-tree change, so commit the generated files
before running the gate.

`just check` is the cheap half of the gate (format, types, lint, web tests). Run
it after each edit batch: a type error or a formatting slip otherwise fails
`just ci` minutes in, after the Rust build.

## Branching

- Work is sequential on the long-lived `dev` branch, and `dev` is pushed
  freely: sharing work-in-progress there needs no approval.
- `dev` -> `main` only when a release is wanted, never per feature. `main`
  stays green: CI's `gate` must pass and the PR is squash-merged. After the
  merge, fast-forward `dev` onto `main` (`git fetch origin && git reset --hard
  origin/main`); a release tag comes from `main`, never from `dev`.
- Still run `just ci` before pushing, so `dev` stays healthy enough to cut a
  release from at any time.
- Parallel work gets its own branch and worktree: `git worktree add ../dandelion-<slug> -b <type>/<slug>`; merge it into `dev` when done.
- Commits follow Conventional Commits (`feat(agents): …`); the commit-msg hook enforces it. Scopes are crate or page names.
- The layout below removes shared hotspot files. Keep it that way: when a change needs a central list, add a registration seam instead.

## Layout

- `crates/<domain>/` — one Rust crate per domain, package `dandelion-<domain>`. `server` owns HTTP, WebSocket, auth, and wiring; domain crates own their logic and expose plain Rust APIs. `core` holds types shared by several crates.
- `web/src/components/<Name>/` — one component per folder: `<Name>.tsx`, `<Name>.test.tsx`, `<Name>.stories.tsx` (Storybook, `just storybook`), and its helpers. `components/ui/` holds generated shadcn primitives (add with `pnpm dlx shadcn add`); app code imports `cn` from `@/lib/utils`.
- `web/src/pages/<screen>/page.tsx` — the metadata and a `lazy()` wrapper around `screen.tsx`, which composes the components; the router suspends while a page loads (ADR-0008). The page registry (`web/src/app/page-registry.ts`) discovers it: routes and navigation never need a manual edit.
- `web/src/api/gen/` — generated from the server's OpenAPI. Regenerate it, never edit it.
- `web/src/lib/`, `web/src/hooks/` — logic shared by several components.
- `web/e2e/` — Playwright specs, run against a scratch server by `just e2e`;
  `scripts/` — shell helpers the recipes call.
- `docs/adr/` — architecture decisions. A change that alters a decision there adds a new ADR that supersedes it.
- `docs/tech/` — verified facts about external systems (herdr API, Claude Code state).

## Code rules

- Rust: workspace lints (clippy pedantic, `unsafe` forbidden) apply to every crate via `[lints] workspace = true`. Errors are `thiserror` types in libraries, `anyhow` only in the binary.
- Web: strict TypeScript, no `any`. Style through Tailwind and the design tokens in `web/src/styles/tokens.css`; follow `docs/design.md`.
- Files stay under 500 lines, and integration test files under 700; split by
  responsibility before that.
- Every mutating API action writes an audit log entry.
- Tests sit next to the code: Rust `#[cfg(test)]` modules or `crates/<domain>/tests/`, web `*.test.ts(x)` beside the source.
