# Local Stack — SQLite/Filesystem Backends + Native Server

Phase 4 of 5 — the `impl-engine` chain. Makes the engine run on a local machine: real
filesystem blob storage, real SQLite metadata storage (via the Phase-3 shared SQL), a
local id-resolver, and the `apps/server` native HTTP entry point exposing the store API.
This is both the dev environment and the substrate integration tests run against (doc02.04).

Triage against the Surfaces of Phase 2 (core-usecases — the `Store`/use-case signatures
and trait set) and Phase 3 (shared-sql — the exported query strings + migration loader).

## Do NOT

- Do NOT duplicate resolution/merge logic — the use-cases own it (Phase 2). The backends
  only execute storage operations via `braincrawl_sql` strings.
- Do NOT write raw SQL inline in the SQLite backend — consume `braincrawl_sql`. If a needed
  query is missing, that is a Phase-3 gap; add it to `braincrawl_sql` (and its test), not inline.
- Do NOT touch the Cloudflare backends or `apps/worker` (Phase 5).
- Do NOT use a multi-threaded runtime. The traits are `?Send` (doc02.04) — the server must
  run on a **current-thread** tokio runtime + `LocalSet`. Use axum with a single-threaded server.
- Do NOT persist bytes for `rights=restricted` content (use-case already enforces; don't bypass).

## Plan

### 1. `crates/backends/blob-fs` — filesystem `BlobStore`

Store blobs under a configurable root dir; `put(key,bytes,mime)` writes `{root}/{key}`
(creating parent dirs), `get` reads + returns `StoredBlob` with a computed `content_hash`
(same hash choice as `blob-mem`), `delete` removes. Map IO errors to `DomainError::Backend`.

### 2. `crates/backends/store-sqlite` — SQLite `MetadataStore` + `PayloadsRepo`

- Add `rusqlite` (bundled). Open a connection from a configurable path; on construction,
  apply `braincrawl_sql` migrations (idempotently — track applied versions or use
  `CREATE TABLE IF NOT EXISTS`-safe application).
- Implement every `MetadataStore`/`PayloadsRepo` method by executing the matching
  `braincrawl_sql` query string. `resolve_live` walks `merged_into` iteratively (with a
  cycle guard) and may path-compress by updating rows. `merge` runs the repoint statements
  in a transaction. Expand IN-list placeholders via the `braincrawl_sql` builder.
- Single-threaded: wrap the `Connection` in `RefCell` (no `Send` needed).

### 3. Local resolver + coordinator

For the local stack, reuse the in-memory `resolver-mem` and no-op `coord-local` from Phase 2
(the SQLite alias UNIQUE constraint is the serialization point; doc02.01.02 §Concurrency).
If a persistent resolver is trivial, a SQLite-backed `IdResolver` may instead be added to
`store-sqlite` — document the choice. Reuse the host `Clock`/`IdGen` from Phase 2.

### 4. `apps/server` — native HTTP entry point

Replace the stub `main` with an axum app on a current-thread runtime (`#[tokio::main(flavor = "current_thread")]`
or a manual `LocalSet`). Wire the local backends into the use-case `Store` and route the
OpenAPI surface (`03-openapi.yaml`):

- `PUT /works` → `put_work`; `GET /works/{id}` → `get_work` (404 on unknown).
- `PUT /works/{id}/content/{kind}` → `put_content`; `GET /works/{id}/content/{kind}` →
  `get_content`, mapping `ContentOutcome` to status 200/302/202/451/404.
- `PUT /edges` → `put_edges`; `GET /works/{id}/edges?dir&cursor&limit` → `get_edges`.
- `POST /works/have` → `have`.

`{id}` arrives as `namespace:value` (e.g. `doi:10.x`, `openalex:W1`, `guid:…`); parse into
`Alias` (split on first `:`). Config (db path, blob root, bind addr) via env vars with sane
defaults; document them.

### 5. Integration tests (`apps/server/tests/` or `crates/backends/store-sqlite/tests/`)

- Backend-level: run the Phase-2 guarantee suite (idempotency, convergence, confluent merge,
  stub creation, edge dedup, rights gating) against `store-sqlite` + `blob-fs` in a tmp dir —
  proving the SQLite backend matches the in-memory semantics.
- HTTP-level smoke: boot the server in-process, exercise `PUT /works` → `GET /works/{id}`,
  `POST /works/have`, and a content put/get round-trip via an HTTP client.

## Files to Modify

- `crates/backends/blob-fs/src/lib.rs`, `crates/backends/blob-fs/Cargo.toml`
- `crates/backends/store-sqlite/src/lib.rs`, `crates/backends/store-sqlite/Cargo.toml` (+ rusqlite, braincrawl-sql)
- `apps/server/src/main.rs` (+ axum/tokio/serde_json deps in its Cargo.toml)
- integration test files (new)
- root `Cargo.toml` — axum/tokio/rusqlite workspace deps as needed

## Verification

```bash
cargo build --workspace
cargo test --workspace
# server smoke: boot and hit `have`
(cargo run -p braincrawl-server & SRV=$!; sleep 3; \
 curl -fsS -X POST localhost:8787/works/have -H 'content-type: application/json' \
   -d '{"ids":["doi:10.0/none"]}' && echo OK; kill $SRV)
```

## Out of Scope

- Cloudflare worker / R2 / D1 / KV / Durable Object (Phase 5).
- Fuzzy fallback, Layer 3 collections.

## Surface after this phase

- `blob-fs` implements `BlobStore` on the filesystem; `store-sqlite` implements
  `MetadataStore` + `PayloadsRepo` via `braincrawl_sql`, applying migrations on open.
- `apps/server` is a runnable axum binary on a current-thread runtime exposing all
  `03-openapi.yaml` routes, wired to local backends; config via documented env vars
  (db path, blob root, bind addr; default port documented).
- The Phase-2 guarantee suite passes against the SQLite+fs stack, and an in-process HTTP
  smoke test passes. `cargo test --workspace` is green.
- Cloudflare backends + `apps/worker` remain `todo!()`/stub, unchanged.
