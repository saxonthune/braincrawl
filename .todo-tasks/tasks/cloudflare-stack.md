# Cloudflare Stack — Worker + R2/D1/KV + Durable Object

Phase 5 of 5 — the `impl-engine` chain. Binds the same engine to Cloudflare edge
primitives (doc02.02.00): the `apps/worker` Wasm entry point, R2/D1/KV backends, and a
Durable-Object-per-work-id coordinator. By construction this reuses core + shared SQL
unchanged — only the backends and entry point differ from the local stack (doc02.04).

Triage against the Surfaces of Phase 2 (core-usecases — `Store`/use-case signatures, the
`Coordinator` trait), Phase 3 (shared-sql — query strings + migrations), and Phase 4
(local-stack — the axum routing shape to mirror, and the `namespace:value` id parsing).

## Do NOT

- Do NOT duplicate use-case or SQL logic — reuse `braincrawl_core::usecases` and
  `braincrawl_sql`. The D1 backend executes the same query strings as `store-sqlite`.
- Do NOT add a `Send` bound or use multi-threaded constructs — Worker futures are `!Send`
  (doc02.04); the `?Send` traits already fit.
- Do NOT break the host build. After adding `workers-rs`, `cargo build --workspace` on the
  host must still succeed for all non-worker crates. Gate worker-only deps to the worker
  crate and, if the worker cannot compile for the host target, set root `default-members`
  to exclude `apps/worker` so `--workspace`/default builds stay green while the worker is
  built explicitly for wasm.
- Do NOT hardcode live binding IDs in `wrangler.toml` — declare bindings by name; real IDs
  are filled at deploy time (the file already says so).
- Do NOT implement fuzzy fallback, Layer 3 per-tenant DBs, Queues, or Vectorize — those are
  noted as later in doc02.02.00 and out of this chain.

## Plan

### 1. `crates/backends/blob-r2` — `BlobStore` over R2

Implement `put`/`get`/`delete` against a `worker::Bucket` handle. `get` returns `StoredBlob`
with `content_hash` (reuse the shared hash). Map errors to `DomainError::Backend`.

### 2. `crates/backends/store-d1` — `MetadataStore` + `PayloadsRepo` over D1

Implement every method using `worker::D1Database` prepared statements with the **same
`braincrawl_sql` query strings** the SQLite backend uses. Use D1 batch/transaction APIs for
`merge`'s multi-statement repoint. Expand IN-lists via the `braincrawl_sql` builder.

### 3. `crates/backends/resolver-kv` — `IdResolver` over KV

`resolve` = KV get on `{namespace}:{value}`; `remember` = KV put. This is the hot-cache
projection (doc02.01.01); D1's `alias` table remains the source of truth.

### 4. `WorkDurableObject` — the `Coordinator` (doc02.02.00)

Implement a Durable Object class keyed by work id that satisfies the core `Coordinator`
trait: serialize the resolve+merge critical section and coalesce concurrent cache-miss
fetches for one work (thundering-herd prevention). The worker constructs a `Coordinator`
impl that routes `with_lock(key)` through the DO stub for that key. Keep the rate-budget
hook present even if minimally implemented; document what is stubbed.

### 5. `apps/worker` — Wasm entry point

Add `workers-rs` (`worker` crate) + `getrandom` with the `js` feature (for `IdGen`). Write
the `#[event(fetch)]` handler: read R2/D1/KV bindings + the DO namespace from `Env`, build the
backends and the use-case `Store`, and route the same OpenAPI surface as `apps/server`
(reuse the `namespace:value` id parsing and `ContentOutcome`→status mapping from Phase 4 —
extract shared routing helpers into a small module if it avoids duplication, otherwise mirror).

### 6. `wrangler.toml`

Uncomment and complete the binding declarations (R2 `BLOB_BUCKET`, D1 `DB`, KV
`ID_RESOLVER_KV`, Durable Object `WORK_DO` → `WorkDurableObject`), add the D1 migrations
config pointing at `migrations/`, and the `build` command (`worker-build` / `cargo install -q worker-build`).
Leave concrete IDs as placeholders.

## Files to Modify

- `crates/backends/blob-r2/src/lib.rs` (+ Cargo.toml: worker)
- `crates/backends/store-d1/src/lib.rs` (+ Cargo.toml: worker, braincrawl-sql)
- `crates/backends/resolver-kv/src/lib.rs` (+ Cargo.toml: worker)
- `apps/worker/src/lib.rs` (+ Cargo.toml: worker, getrandom js, braincrawl-sql)
- `apps/worker/wrangler.toml`
- root `Cargo.toml` (worker workspace dep; possibly `default-members`)

## Verification

```bash
# Host build of everything except the worker must stay green.
cargo build --workspace --exclude braincrawl-worker
cargo test --workspace --exclude braincrawl-worker
# Worker compiles to the edge target.
rustup target add wasm32-unknown-unknown
cargo build -p braincrawl-worker --target wasm32-unknown-unknown
```

## Out of Scope

- Live deploy / real binding IDs, Queues, Vectorize, per-tenant L3 DBs (doc02.02.00 "later").
- Fuzzy fallback, Layer 3 collections.

## Surface after this phase

- `blob-r2`, `store-d1`, `resolver-kv` fully implement their traits against R2/D1/KV,
  with `store-d1` executing the shared `braincrawl_sql` strings.
- A `WorkDurableObject` implements the `Coordinator` seam (per-work single-writer +
  cache-miss coalescing); the worker binds it.
- `apps/worker` is a `workers-rs` fetch handler routing the full `03-openapi.yaml` surface,
  wired to the Cloudflare backends; `wrangler.toml` declares all bindings + D1 migrations by name.
- Host `cargo build/test --workspace --exclude braincrawl-worker` is green and the worker
  builds for `wasm32-unknown-unknown`. Core and `braincrawl_sql` are unchanged from earlier phases.
