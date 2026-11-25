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

# Expose loopback Dandelion on the tailnet through Tailscale's HTTPS proxy.
# `serve` writes host configuration, so it needs root; a sudo prompt is
# expected. Dandelion itself never changes host networking.
tailnet-serve:
    sudo tailscale serve --bg 7443

# One-time alternative: let the current user manage serve config without sudo.
# After this, plain `tailscale serve --bg 7443` works.
tailnet-operator:
    sudo tailscale set --operator=$USER

# Install the sudoers drop-in generated from <data-dir>/sudo-allowlist.toml.
# Run it again after changing the allowlist and restart the server. Needs a
# password once. Pass `--data-dir /path` when the server does not use the default.
install-sudoers *args:
    mkdir -p target
    cargo run --quiet --locked -p dandelion-server -- sudoers {{args}} > target/dandelion-services.sudoers
    sudo visudo -cf target/dandelion-services.sudoers
    sudo install -m 0440 -o root -g root target/dandelion-services.sudoers /etc/sudoers.d/dandelion-services
    sudo visudo -c

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

# Regenerate the OpenAPI document and its TypeScript client.
# dlx runs the generator with its own TypeScript 5; the app builds with TypeScript 7,
# which no longer ships the compiler API the generator needs.
gen:
    mkdir -p target web/src/api/gen
    cargo run --quiet --locked -p dandelion-server -- openapi > target/openapi.json
    pnpm dlx openapi-typescript@7.13.0 target/openapi.json -o web/src/api/gen/schema.d.ts
    cargo run --quiet --locked -p dandelion-server -- ts-types web/src/api/gen/ws

# Fail when generated files are stale, missing, orphaned, or untracked.
# Regenerating from an empty directory makes removed types leave no orphans.
gen-check:
    rm -rf web/src/api/gen
    just gen
    @test -z "$(git status --porcelain -- web/src/api/gen)"

# Run all tests.
test:
    cargo nextest run --workspace --locked --no-tests=warn
    {{web}} test

# Dependency policy: licenses, advisories, sources.
deny:
    cargo deny check

# Production build: the release bundle plus the Storybook check.
build: release
    {{web}} build-storybook --quiet --output-dir dist-storybook

# Production build without the Storybook check; used by install and deploy.
release:
    {{web}} build
    cargo build --release --locked
    ./target/release/dandelion assets

# Install the systemd user units; run by install and deploy.
install-units:
    install -Dm0644 packaging/dandelion.service ~/.config/systemd/user/dandelion.service
    install -Dm0644 packaging/dandelion-backup.service ~/.config/systemd/user/dandelion-backup.service
    install -Dm0644 packaging/dandelion-backup.timer ~/.config/systemd/user/dandelion-backup.timer
    systemctl --user daemon-reload

# Install (or reinstall) the systemd user service; keeps data and settings.
install: release install-units
    install -Dm0755 target/release/dandelion ~/.local/bin/dandelion
    install -d -m0700 ~/.config/dandelion
    test -f ~/.config/dandelion/env || install -m0600 packaging/env.example ~/.config/dandelion/env
    systemctl --user enable dandelion.service
    systemctl --user restart dandelion.service
    systemctl --user enable --now dandelion-backup.timer
    curl -fsS --retry 10 --retry-connrefused --retry-delay 1 http://127.0.0.1:7443/api/health
    @echo
    @loginctl show-user $USER -p Linger --value | grep -qx yes || echo "before a reboot works: sudo loginctl enable-linger $USER"
    @echo "Optional: just install-sudoers for system-unit actions; restart the service after changing the allowlist."

# Upgrade an installed server: rebuild, back up, replace the binary, restart.
deploy: release install-units
    @set -a; [ -r ~/.config/dandelion/env ] && . ~/.config/dandelion/env; set +a; ./target/release/dandelion backup
    install -Dm0755 target/release/dandelion ~/.local/bin/dandelion
    systemctl --user restart dandelion.service
    curl -fsS --retry 10 --retry-connrefused --retry-delay 1 http://127.0.0.1:7443/api/health
    @echo

# Remove the units and the binary; keeps data and settings.
uninstall:
    -systemctl --user disable --now dandelion-backup.timer
    -systemctl --user disable --now dandelion.service
    rm -f ~/.config/systemd/user/dandelion.service ~/.config/systemd/user/dandelion-backup.service ~/.config/systemd/user/dandelion-backup.timer
    -systemctl --user daemon-reload
    rm -f ~/.local/bin/dandelion
    @echo "Kept: the data directory and ~/.config/dandelion/env"

# Write a database backup now, with the env file the service reads.
backup *args:
    @set -a; [ -r ~/.config/dandelion/env ] && . ~/.config/dandelion/env; set +a; ~/.local/bin/dandelion backup {{args}}

# The full gate. CI runs exactly this; run it before every push.
ci: fmt-check typecheck gen-check lint test deny build
