#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORKER_DIR="$REPO_ROOT/apps/worker"
TEST_TOKEN="conformance-test-token-local"

cleanup() {
    if [[ -n "${WRANGLER_PID:-}" ]]; then
        kill "$WRANGLER_PID" 2>/dev/null || true
    fi
}
trap cleanup EXIT

# 1. Inject the test secret for wrangler dev --local
cat > "$WORKER_DIR/.dev.vars" <<EOF
AUTH_TOKEN=$TEST_TOKEN
EOF

# 2. Apply D1 migrations locally (wrangler reads wrangler.toml from cwd)
(cd "$WORKER_DIR" && wrangler d1 migrations apply braincrawl-db --local)

# 3. Start wrangler dev in the background
(cd "$WORKER_DIR" && wrangler dev --local --port 8787) &
WRANGLER_PID=$!

# 4. Readiness probe: poll POST /works/have until the worker responds
echo "Waiting for worker to become ready..."
for i in $(seq 1 60); do
    status=$(curl -s -o /dev/null -w "%{http_code}" \
        -X POST http://127.0.0.1:8787/works/have \
        -H "Authorization: Bearer $TEST_TOKEN" \
        -H "Content-Type: application/json" \
        -d '{"ids":[]}' 2>/dev/null || echo "000")
    if [[ "$status" == "200" ]]; then
        echo "Worker ready after ${i}s."
        break
    fi
    sleep 1
done
if [[ "$status" != "200" ]]; then
    echo "Worker did not become ready in time (last status: $status)" >&2
    exit 1
fi

# 5. Run the conformance suite
BRAINCRAWL_CONFORMANCE_URL=http://127.0.0.1:8787 \
BRAINCRAWL_CONFORMANCE_TOKEN="$TEST_TOKEN" \
    cargo test -p braincrawl-conformance --test external -- --nocapture

# 6. Negative auth check: request without token must return 401
unauth_status=$(curl -s -o /dev/null -w "%{http_code}" \
    -X POST http://127.0.0.1:8787/works/have \
    -H "Content-Type: application/json" \
    -d '{"ids":[]}')
if [[ "$unauth_status" != "401" ]]; then
    echo "Expected 401 without token, got $unauth_status" >&2
    exit 1
fi
echo "Negative auth check passed (got 401 without token)."
