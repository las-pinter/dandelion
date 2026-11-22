# Every command humans, agents, and CI use. Run `just` to list them.

set shell := ["bash", "-euo", "pipefail", "-c"]
export COREPACK_ENABLE_DOWNLOAD_PROMPT := "0"

web := "pnpm --dir web"

default:
    @just --list --unsorted

# One-time setup after cloning (or in a new worktree).
setup:
    {{web}} install --frozen-lockfile
    {{web}} exec lefthook install
    cargo fetch --locked

# Run the web dev server (proxies /api to the Rust server).
dev-web:
    {{web}} dev

# Browse and build components in isolation.
storybook:
    {{web}} storybook

# Format everything.
fmt:
    cargo fmt --all
    {{web}} format

# Check formatting without writing.
fmt-check:
    cargo fmt --all --check
    {{web}} format:check

# Lint Rust (clippy, warnings are errors) and web (oxlint).
lint:
    cargo clippy --workspace --all-targets --locked -- -D warnings
    {{web}} lint

# Type-check the web app.
typecheck:
    {{web}} typecheck

# Run all tests.
test:
    cargo nextest run --workspace --locked --no-tests=warn
    {{web}} test

# Dependency policy: licenses, advisories, sources.
deny:
    cargo deny check

# Production build: web bundle, Storybook (as a check), then the release binary.
build:
    {{web}} build
    {{web}} build-storybook --quiet --output-dir dist-storybook
    cargo build --release --locked

# The full gate. CI runs exactly this; run it before every push.
ci: fmt-check typecheck lint test deny build
