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
`just dev-web` proxies `/api` there. Non-loopback bind addresses are refused
until authentication and TLS exist.

Contributor and agent conventions: [`AGENTS.md`](AGENTS.md).
Architecture decisions: [`docs/adr/`](docs/adr/).

## License

[MIT](LICENSE)
