# Worker endpoint parity — /graph/neighborhood + /stats

## Motivation

The Cloudflare Worker (`apps/worker/src/lib.rs`) claims in its header comment to
"route the same OpenAPI surface as `apps/server`" but it does not: it routes only
`POST /works/have`, `PUT /works`, `PUT /edges`, and the `GET/PUT /works/*` family.
It is **missing `POST /graph/neighborhood` and `GET /stats`**, both of which the
native server (`apps/server/src/lib.rs`) already exposes and both of which are
already implemented as core use-cases (`crates/core/src/usecases.rs`:
`Store::neighborhood` at ~line 304, `Store::stats` at ~line 444). A CLI session
pointed at a deployed Worker would get 404 for `braincrawl neighborhood` and
`braincrawl stats`. This phase closes that gap so the Worker is a genuine peer of
the server.

`/jobs` is intentionally NOT part of this phase — it needs a Queues backend and a
consumer that does not exist yet.

## Do NOT

- Do NOT touch `crates/core` — `Store::neighborhood` and `Store::stats` already
  exist and are correct. This is pure routing/handler wiring in the Worker crate.
- Do NOT add `/jobs`, Queues, or any async-ingestion path. Out of scope.
- Do NOT change the DO coordinator, auth, or any backend crate.
- Do NOT change `apps/server` — it already has these endpoints; the Worker is
  copying its behavior, not the other way around.
- Do NOT change the request/response JSON shapes. The Worker's handlers must
  produce byte-identical JSON to the server's `handler_neighborhood` and
  `handler_stats` so one conformance suite passes against both.

## Plan

### 1. Add the neighborhood request type

In `apps/worker/src/lib.rs`, mirror the server's `NeighborhoodHttpRequest`
(`apps/server/src/lib.rs:156`): a `#[derive(Deserialize)]` struct with
`seeds: Vec<String>`, `dir: String`, `depth: u32`, `max_nodes: u32`.

### 2. Add `handle_neighborhood`

Mirror the server's `handler_neighborhood` (`apps/server/src/lib.rs:376`):
- Parse `dir` → `EdgeDir` (`"forward"` / `"backward"`, else 400 "dir must be
  forward or backward").
- Map `seeds` through `parse_alias` (the Worker already has this helper).
- Call `store.neighborhood(seeds, dir, depth, max_nodes).await`.
- On `Ok`, `Response::from_json(&neighborhood)`; on `Err`, `err_response(&e)`.
- `EdgeDir` is already imported in the Worker; add no new core imports beyond
  what the call needs.

### 3. Add `handle_stats`

Mirror the server's `handler_stats` (`apps/server/src/lib.rs:359`): call
`store.stats().await`, on `Ok` return `Response::from_json(&stats)`, on `Err`
`err_response(&e)`.

### 4. Wire both into the `main` fetch router

In the `#[event(fetch)]` `main` function (`apps/worker/src/lib.rs:365`), after the
auth gate and `build_store`, add route matches BEFORE the `/works/*` prefix block:
- `POST /graph/neighborhood` → `handle_neighborhood`
- `GET /stats` → `handle_stats`

Keep the existing exact-match-before-wildcard ordering style already used for
`/works/have`, `/works`, `/edges`.

## Files to Modify

- `apps/worker/src/lib.rs` — add `NeighborhoodHttpRequest`, `handle_neighborhood`,
  `handle_stats`, and two route matches in `main`. Update the module-header
  comment's route list to include the two new routes so it stops lying.

## Verification

```bash
cargo build -p braincrawl-worker --target wasm32-unknown-unknown
cargo build
```

## Out of Scope

- `POST /jobs` and any Queues / async ingestion (separate future work).
- Real DO locking and KV-allowlist auth (separate filed drafts).

## Notes

- The Worker is excluded from `default-members` (root `Cargo.toml`), so the
  primary build check is the explicit `--target wasm32-unknown-unknown` command.
  `wasm32-unknown-unknown` is installed in this environment.
- `worker-build`/`wrangler` are NOT required for this phase's verification — a
  plain `cargo build` to wasm is enough to prove the routing compiles.

## Surface after this phase

- `apps/worker/src/lib.rs` exposes, in addition to the existing routes:
  - `POST /graph/neighborhood` — body `{seeds: string[], dir, depth, max_nodes}`,
    returns the neighborhood subgraph JSON (`{nodes, edges, truncated}`),
    behaviorally identical to the native server's `/graph/neighborhood`.
  - `GET /stats` — returns the aggregate `GraphStats` JSON, identical shape to the
    server's `/stats`.
- A `NeighborhoodHttpRequest` deserialize struct and `handle_neighborhood` /
  `handle_stats` async fns exist in the Worker crate.
- The Worker still does NOT route `/jobs`. The DO coordinator lock is still the
  advisory stub. Auth is still the single `AUTH_TOKEN` shared secret. None of
  those changed in this phase.
- The Worker still builds only for `wasm32-unknown-unknown` and is still excluded
  from `default-members`.
