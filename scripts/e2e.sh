#!/usr/bin/env bash
# Runs the Playwright smoke suite against a scratch Dandelion.
# The port defaults to 7444; set E2E_PORT to move the scratch server.
#
# The server's own startup prints the setup link; the suite reads it from the
# log. Everything lives under target/, so a run leaves the real data alone.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

port="${E2E_PORT:-7444}"
export E2E_PORT="$port"

rm -rf target/e2e
mkdir -p target/e2e

DANDELION_PUBLIC_URL="http://localhost:$port" \
  ./target/debug/dandelion serve --bind "127.0.0.1:$port" --data-dir target/e2e/data \
  > target/e2e/server.log 2>&1 &
server=$!
trap 'kill "$server" 2>/dev/null || true' EXIT

ready=""
for _ in $(seq 1 100); do
  if ! kill -0 "$server" 2>/dev/null; then
    echo "the scratch server exited; see target/e2e/server.log" >&2
    exit 1
  fi
  if curl -fsS "http://127.0.0.1:$port/api/health" > /dev/null 2>&1; then
    ready=1
    break
  fi
  sleep 0.2
done
if [ -z "$ready" ]; then
  echo "the scratch server did not become healthy; see target/e2e/server.log" >&2
  exit 1
fi

pnpm --dir web exec playwright test "$@"
