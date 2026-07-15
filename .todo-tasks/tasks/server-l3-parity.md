# Server: L3 doc + agent-file endpoints at parity with the worker

## Motivation

DECIDED: the Cloudflare worker and the native server implement the same
contract. The worker now serves `/api/l3/docs*`, `/api/l3/agent/*`, and an
R2-backed `/api/l3/graph`; the native server has only its fs-based graph
endpoint. Bring the server to parity over its L3 root directory, sharing
one normalize pipeline, and make the conformance suite enforce L3 parity
the way it enforces the core store surface. After this, a client is
backend-agnostic: base URL is the only difference, and `l3 push/pull`
works against either end.

## Do NOT

- Do NOT change the wire contract in any way — response shapes, status
  codes (200/204/400+warnings/404/409/413), query params (`force=1`), and
  normalization semantics must match `apps/worker/src/l3.rs` exactly.
  In particular preserve the merged nuance: `heading without anchor`
  warnings NEVER block a PUT (anchor assignment resolves them); all other
  warning kinds block unless `force=1`.
- Do NOT make `crates/l3` depend on axum, worker, tokio, or any HTTP/IO
  crate — the shared pipeline is pure string logic.
- Do NOT touch the CLI (`l3 push/pull` already speaks this contract; it
  gains local-server support for free).
- Do NOT implement store-to-store sync (`--from/--to`) — separate task.
- Do NOT weaken the worker's behavior while refactoring it onto the
  shared pipeline; `./scripts/worker-conformance.sh` must still pass.

## Plan

### 1. Extract the normalize pipeline into crates/l3

New `crates/l3/src/normalize.rs`, exported from lib.rs:

```rust
pub enum NormalizeOutcome {
    Normalized { content: String, assigned: Vec<Assigned> },
    BlockingWarnings(Vec<Warning>),
    AnchorConflicts(Vec<String>),   // sorted
}
pub fn normalize_doc(
    slug: &str,
    body: &str,
    other_docs: &[(String, String)],
    today: &str,     // yyyy-mm-dd, caller supplies (clock differs per runtime)
    force: bool,
) -> NormalizeOutcome
```

Move the logic currently inline in `handle_put_doc`
(`apps/worker/src/l3.rs` lines ~105-137): parse_sources on the incoming
doc; filter warnings, treating `heading without anchor` as non-blocking;
if blocking warnings and !force → `BlockingWarnings`; collect_anchors
over `other_docs`; incoming-anchor ∩ others → `AnchorConflicts`;
assign_ids_source; upsert `updated:` with `today` → `Normalized`.
Unit-test it in the crate (blocked write, forced write, conflict, happy
path with anchor assignment + updated stamp).

Refactor `handle_put_doc` in the worker to call it (worker keeps: slug
validation, force parsing, R2 load of other docs, JSON/status mapping,
R2 write).

### 2. Server doc endpoints

In `apps/server/src/handlers/l3.rs` (extend), all inside the authed
router, all 404 when `state.l3_root` is `None` (same as the graph
handler):

- `GET /api/l3/docs` — walk the root for `*.l3.md` (skip `_`-prefixed
  and `INDEX.md`, same rules as `l3::parse`), return
  `[{doc, size, modified}]` (mtime as RFC3339).
- `GET /api/l3/docs/{slug}` — slug charset `[a-z0-9-]+` (400), read
  `<root>/<slug>.l3.md`, `text/markdown; charset=utf-8`, 404 if absent.
- `PUT /api/l3/docs/{slug}` — read body; load all other docs from the
  root; call `normalize_doc` (today from chrono/std time, matching the
  server's existing date handling); map outcomes to the same statuses/
  JSON bodies as the worker; write the file; respond with the normalized
  markdown. Writing the file must be visible to the existing fs watcher
  so `/api/events` fires — verify the watcher watches the root
  recursively enough; if the write needs a temp-file+rename to avoid a
  partial-read race with the watcher's debounce, do that.
- Use `spawn_blocking` for fs work, following the graph handler's
  pattern.

### 3. Server agent-file endpoints

Same file: `GET /api/l3/agent` (list), `GET /api/l3/agent/{name}` (raw
markdown | 404), `PUT /api/l3/agent/{name}` (204; 400 bad name; 413 over
64 KB) — backed by `<root>/_agent/<name>.md`, creating `_agent/` on first
write. Name charset `[a-z0-9-]+`. No parsing, no stamping; excluded from
docs list and graph by the existing `_`-prefix skip rules.

### 4. Routes

Register in `apps/server/src/lib.rs` next to the existing
`/api/l3/graph` route (authed section). Route table after this task
matches the worker's `/api/l3/*` surface 1:1.

### 5. Conformance promotion

Find where the native server runs the conformance suite today (grep for
`run_all` callers under `apps/server*`/`crates/conformance`). Extend that
harness so, when the server under test has an L3 root configured (use a
tempdir), it also runs the existing `check_l3_docs` and
`check_l3_agent_files` from `crates/conformance/src/lib.rs` — unmodified;
they were written against the worker and must pass verbatim against the
server. If the server conformance harness cannot configure an L3 root,
add a dedicated integration test in `apps/server` that spins the router
with a temp root and runs both checks against it over HTTP.

## Files to Modify

- `crates/l3/src/normalize.rs` — new; `crates/l3/src/lib.rs` — export
- `apps/worker/src/l3.rs` — refactor PUT onto `normalize_doc`
- `apps/server/src/handlers/l3.rs` — doc + agent handlers
- `apps/server/src/lib.rs` — routes
- server conformance/integration test — L3 checks against the server
- `crates/conformance/src/lib.rs` — only if a check needs a
  server-vs-worker-neutral tweak (avoid if possible; note it if done)

## Verification

```bash
cargo test -p l3
cargo test --workspace
./scripts/worker-conformance.sh
```

## Out of Scope

- Store-to-store sync (`l3 sync --from <url> --to <url>`) — follow-up
  task, filed separately
- Any CLI change
- SSE/events on the worker; /jobs on the worker

## Notes

- The wire contract source of truth is the merged `apps/worker/src/l3.rs`
  plus `check_l3_docs`/`check_l3_agent_files` — when in doubt, make the
  server satisfy the existing checks rather than reinterpreting the spec.
- Server writes via PUT should surface in `/api/events` (the watcher owns
  that); do not hand-emit events.

## Surface after this phase

- Native server serves `/api/l3/docs`, `/api/l3/docs/{slug}` (GET/PUT),
  `/api/l3/agent`, `/api/l3/agent/{name}` (GET/PUT) with byte-parity
  semantics to the worker, over `<l3_root>` files.
- `braincrawl_l3` (package `l3`) exports
  `normalize_doc(slug, body, other_docs, today, force) -> NormalizeOutcome`;
  worker and server both use it.
- Conformance: `check_l3_docs` + `check_l3_agent_files` run against BOTH
  backends in their respective gates.
- Negative space: wire contract unchanged from the worker's current
  behavior; CLI untouched; `l3 push/pull` now works against a local
  server URL with no code change.
