#!/usr/bin/env bash
set -euo pipefail

# Self-contained edge gate: boots the Worker under `wrangler dev` against an
# isolated, freshly-wiped local D1/R2/KV state, runs the shared conformance suite
# against it, and tears the whole process group down. It never touches the shared
# braincrawl server (default port 8787) or its corpus.

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORKER_DIR="$REPO_ROOT/apps/worker"
TEST_TOKEN="conformance-test-token-local"
# Never 8787 — that's the shared dev server holding the real corpus.
PORT="${WORKER_TEST_PORT:-8799}"
BASE="http://127.0.0.1:$PORT"
# Isolated emulator state, wiped each run so the suite's `stats == 0` precondition
# holds and so we never read or mutate the shared server's data.
PERSIST_DIR="$WORKER_DIR/.wrangler-conformance-state"
LOG="$WORKER_DIR/.wrangler-conformance.log"
# wrangler's `[build]` step is a `cargo build`, which locks the *entire* target
# dir. Sharing target/ with the host `cargo test` below makes the test block on
# that lock indefinitely. A separate target dir gives each its own lock.
export CARGO_TARGET_DIR="$REPO_ROOT/target/worker-wasm"

WRANGLER_PGID=""
cleanup() {
    if [[ -n "$WRANGLER_PGID" ]]; then
        kill -TERM -"$WRANGLER_PGID" 2>/dev/null || true
    fi
}
trap cleanup EXIT

# 0. wrangler.toml is gitignored (real deploy IDs); fresh checkouts bootstrap
#    from the tracked template. Placeholder IDs are fine for --local runs.
if [[ ! -f "$WORKER_DIR/wrangler.toml" ]]; then
    cp "$WORKER_DIR/wrangler.toml.example" "$WORKER_DIR/wrangler.toml"
fi

# 0b. `[assets].directory` points at web/dist; wrangler dev refuses to start if
#     it's missing (fresh checkout, never built). Build it once rather than
#     making the config existence-tolerant.
if [[ ! -d "$REPO_ROOT/web/dist" ]]; then
    (cd "$REPO_ROOT/web" && pnpm install && pnpm build)
fi

# 1. Fresh emulator state.
rm -rf "$PERSIST_DIR"

# 2. Apply D1 migrations into the isolated state.
(cd "$WORKER_DIR" && wrangler d1 migrations apply braincrawl-db --local --persist-to "$PERSIST_DIR")

# 2b. Seed the test token into the isolated AUTH_KV allowlist.
HASH=$(printf '%s' "$TEST_TOKEN" | sha256sum | cut -d' ' -f1)
(cd "$WORKER_DIR" && wrangler kv key put --binding AUTH_KV "$HASH" '{"tenant":"default","status":"active"}' --local --persist-to "$PERSIST_DIR")

# 4. Start wrangler dev in its own process group so cleanup can reap the whole
#    tree (wrangler + workerd children) and never leak the port.
setsid bash -c "cd '$WORKER_DIR' && exec wrangler dev --local --port $PORT --persist-to '$PERSIST_DIR'" \
    >"$LOG" 2>&1 &
WRANGLER_PGID=$!

# 5. Readiness probe: poll POST /works/have until the worker responds.
echo "Waiting for worker to become ready on $BASE ..."
status="000"
for _ in $(seq 1 60); do
    status=$(curl -s -o /dev/null -w "%{http_code}" \
        -X POST "$BASE/works/have" \
        -H "Authorization: Bearer $TEST_TOKEN" \
        -H "Content-Type: application/json" \
        -d '{"ids":[]}' 2>/dev/null || echo "000")
    if [[ "$status" == "200" ]]; then
        echo "Worker ready."
        break
    fi
    sleep 1
done
if [[ "$status" != "200" ]]; then
    echo "Worker did not become ready in time (last status: $status). Log:" >&2
    tail -30 "$LOG" >&2 || true
    exit 1
fi

# 6. Run the conformance suite against the live worker.
BRAINCRAWL_CONFORMANCE_URL="$BASE" \
BRAINCRAWL_CONFORMANCE_TOKEN="$TEST_TOKEN" \
    cargo test -p braincrawl-conformance --test external -- --nocapture

# 7. Negative auth check: request without token must return 401.
unauth_status=$(curl -s -o /dev/null -w "%{http_code}" \
    -X POST "$BASE/works/have" \
    -H "Content-Type: application/json" \
    -d '{"ids":[]}')
if [[ "$unauth_status" != "401" ]]; then
    echo "Expected 401 without token, got $unauth_status" >&2
    exit 1
fi
echo "Negative auth check passed (got 401 without token)."
