#!/usr/bin/env bash
# Runs the Playwright smoke suite against a scratch Dandelion.
# The port defaults to 7444; set E2E_PORT to move the scratch server.
#
# The server's own startup prints the setup link; the suite reads it from the
# log. Everything lives under target/, so a run leaves the real data alone.
set -euo pipefail

# The server log holds the single-use setup link; keep it private.
umask 077

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

port="${E2E_PORT:-7444}"
export E2E_PORT="$port"

rm -rf target/e2e
mkdir -p target/e2e

# A fixture collection for the sandbox test. The page tries to reach a public
# API route and reports the verdict from inside its own frame. The server runs
# with a scratch HOME, so the manifest root sits under it and the test does
# not depend on where the checkout lives.
scratch_home="$root/target/e2e/home"
docs_root="$scratch_home/documents"
mkdir -p "$docs_root/d" target/e2e/plugins
cat > "$docs_root/d/sandbox-probe.html" <<'HTML'
<!doctype html>
<html lang="en">
  <head><meta charset="utf-8"><title>Sandbox probe</title></head>
  <body>
    <p>Sandbox probe</p>
    <p data-testid="verdict">checking</p>
    <script>
      const verdict = document.querySelector('[data-testid="verdict"]')
      fetch('/api/health')
        .then(() => { verdict.textContent = 'REACHED' })
        .catch(() => { verdict.textContent = 'BLOCKED' })
    </script>
  </body>
</html>
HTML
printf '# Probe notes\n\nA fixture document.\n' > "$docs_root/d/probe-notes.md"
cat > target/e2e/plugins/probe-visuals.json <<JSON
{
  "schema": 1,
  "id": "probe-visuals",
  "name": "Probe visuals",
  "root": "$docs_root",
  "include": ["**/*.{html,md}"],
  "group": []
}
JSON

HOME="$scratch_home" \
  DANDELION_PUBLIC_URL="http://localhost:$port" \
  DANDELION_PLUGINS_DIR="$root/target/e2e/plugins" \
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
