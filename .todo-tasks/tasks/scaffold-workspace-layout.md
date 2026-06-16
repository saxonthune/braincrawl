# Scaffold the Cargo Workspace Layout

## Motivation

The architecture is settled in `.rhidoc/02-architecture/04-monorepo-rust-runtimes.md`
(`doc02.04`): one Cargo workspace in Rust, a platform-agnostic `core` defining port
traits, concrete adapters per technology, and two composition roots (a Wasm Worker and a
native server). The repo is greenfield — no Cargo files exist. This task lays down the
directory tree and crate skeletons so later tasks implement adapters against a stable
structure. Scaffold depth: **skeleton + trait stubs** — real port traits and domain types
in `core`, adapters as structs that `impl` the traits with `todo!()` bodies, and **no
infrastructure dependencies yet** (no `workers-rs`, no `sqlx`). Those land when each
adapter is actually implemented.

## Do NOT

- Do NOT add `workers-rs`, `worker`, `sqlx`, `rusqlite`, `tokio`, or any Cloudflare/DB
  crate as a dependency. Adapter bodies are `todo!()`; they need no runtime yet.
- Do NOT write real adapter logic, SQL drivers, HTTP routing, or a fetch handler. Bodies
  are `todo!()` / placeholders.
- Do NOT put any Cloudflare or platform type in `core`. `core` depends only on
  `async-trait` and `thiserror`.
- Do NOT name a concrete adapter anywhere in `core`. The dependency arrow points inward.
- Do NOT touch `.rhidoc/`, `GOALS.md`, `CLAUDE.md`, or `.todo-tasks/`.
- Do NOT make `apps/worker` depend on `workers-rs`. Keep it a plain `cdylib`+`rlib` stub
  that `cargo check`s natively.

## Plan

### 1. Workspace root `Cargo.toml`

Create `/Cargo.toml`:

```toml
[workspace]
resolver = "2"
members = [
  "crates/core",
  "crates/sql",
  "crates/adapters/blob-r2",
  "crates/adapters/blob-fs",
  "crates/adapters/blob-mem",
  "crates/adapters/store-d1",
  "crates/adapters/store-sqlite",
  "crates/adapters/store-mem",
  "crates/adapters/resolver-kv",
  "apps/worker",
  "apps/server",
]

[workspace.package]
edition = "2021"
version = "0.1.0"

[workspace.dependencies]
core = { path = "crates/core", package = "braincrawl-core" }
async-trait = "0.1"
thiserror = "2"
```

Also create `/.gitignore` containing `/target` (append if a root `.gitignore` already exists — it does not currently).

### 2. `crates/core` — domain types + port traits (THE real code)

`crates/core/Cargo.toml`:

```toml
[package]
name = "braincrawl-core"
edition.workspace = true
version.workspace = true

[dependencies]
async-trait = { workspace = true }
thiserror = { workspace = true }
```

Create these modules under `crates/core/src/`. Put `#![allow(dead_code)]` at the top of `lib.rs`.

- `lib.rs` — `pub mod types; pub mod ports;` and module docs pointing at `doc02.04` / `doc02.01.01`.
- `types.rs`:
  - `pub struct WorkId(pub String);`
  - `pub enum Kind { Abstract, Fulltext }`
  - `pub enum Rights { Open, LinkOnly, Restricted }`
  - `pub struct StoredBlob { pub bytes: Vec<u8>, pub mime: String, pub content_hash: String }`
  - `pub struct PayloadDescriptor { pub key: String, pub kind: Kind, pub version: u32, pub rights: Rights, pub mime: String }`
  - `#[derive(thiserror::Error, Debug)] pub enum DomainError { ... }` with at least a `#[error("rights violation: {0:?}")] RightsViolation(Rights)` and a generic `#[error("not found")] NotFound` variant.
- `ports.rs` — define each as `#[async_trait::async_trait(?Send)]` traits (the `?Send` bound is the deliberate decision from `doc02.04`; the edge runtime is single-threaded). Method bodies are in the trait (signatures only). Use `Result<_, DomainError>` returns:
  - `BlobStore`: `async fn put(&self, key: &str, bytes: Vec<u8>, mime: &str) -> Result<(), DomainError>;` `async fn get(&self, key: &str) -> Result<Option<StoredBlob>, DomainError>;` `async fn delete(&self, key: &str) -> Result<(), DomainError>;` — doc comment: opaque key → bytes, knows nothing of `kind`/`version`.
  - `PayloadsRepo`: `async fn current_payload(&self, id: &WorkId, kind: Kind) -> Result<Option<PayloadDescriptor>, DomainError>;` `async fn next_version(&self, id: &WorkId, kind: Kind) -> Result<u32, DomainError>;` `async fn record(&self, descriptor: &PayloadDescriptor) -> Result<(), DomainError>;`
  - `MetadataStore`: a minimal stub — `async fn upsert_stub(&self, id: &WorkId) -> Result<(), DomainError>;` `async fn edges_out(&self, id: &WorkId) -> Result<Vec<WorkId>, DomainError>;` (doc comment: the queryable graph facts; expands in a later task).
  - `IdResolver`: `async fn resolve(&self, namespace: &str, value: &str) -> Result<Option<WorkId>, DomainError>;` `async fn remember(&self, canonical: &WorkId, namespace: &str, value: &str) -> Result<(), DomainError>;`

### 3. Adapter crates (struct + `impl … { todo!() }`)

Each crate: `Cargo.toml` depends on `core = { workspace = true }` and `async-trait = { workspace = true }` only. `lib.rs` starts with `#![allow(dead_code)]`, defines one struct, and `impl`s the matching port trait with `#[async_trait::async_trait(?Send)]` and every method body `todo!("implement against <tech>")`. Add a one-line module doc naming the backing technology.

| Crate dir | Struct | Port impl'd | Tech (in doc/todo only) |
|---|---|---|---|
| `crates/adapters/blob-r2` | `R2BlobStore` | `BlobStore` | Cloudflare R2 |
| `crates/adapters/blob-fs` | `FsBlobStore` | `BlobStore` | local filesystem |
| `crates/adapters/blob-mem` | `MemBlobStore` | `BlobStore` | in-memory (tests) |
| `crates/adapters/store-d1` | `D1Store` | `PayloadsRepo` + `MetadataStore` | Cloudflare D1 |
| `crates/adapters/store-sqlite` | `SqliteStore` | `PayloadsRepo` + `MetadataStore` | local SQLite |
| `crates/adapters/store-mem` | `MemStore` | `PayloadsRepo` + `MetadataStore` | in-memory (tests) |
| `crates/adapters/resolver-kv` | `KvResolver` | `IdResolver` | Cloudflare KV |

Crate package names: prefix with `braincrawl-` (e.g. `braincrawl-blob-r2`) to avoid collisions; dir names stay as above.

### 4. `crates/sql` — shared schema/query home

`crates/sql/Cargo.toml` (package `braincrawl-sql`, no deps beyond edition/version). `lib.rs`: `#![allow(dead_code)]` and a module doc stating this crate holds the SQLite-dialect query strings shared by `store-d1` and `store-sqlite` (per `doc02.04`). Add `pub const MIGRATIONS: &str = "migrations";` as a placeholder pointer. No real queries yet.

### 5. `migrations/0001_init.sql`

One starter migration drawn from the `payloads` table in `doc02.01.01`. Plain SQLite DDL:

```sql
-- payloads: one row per stored blob; the source of truth for blob keys + rights.
CREATE TABLE payloads (
  canonical_id TEXT NOT NULL,
  kind         TEXT NOT NULL,            -- 'abstract' | 'fulltext'
  version      INTEGER NOT NULL,
  r2_key       TEXT NOT NULL,            -- {canonical_id}/{kind}/v{version}
  content_hash TEXT NOT NULL,
  byte_size    INTEGER NOT NULL,
  mime         TEXT NOT NULL,
  rights       TEXT NOT NULL,            -- 'open' | 'link_only' | 'restricted'
  source       TEXT,
  source_url   TEXT,
  fetched_at   TEXT NOT NULL,
  is_current   INTEGER NOT NULL DEFAULT 1,
  PRIMARY KEY (canonical_id, kind, version)
);
CREATE INDEX payloads_current ON payloads (canonical_id, kind) WHERE is_current = 1;
```

### 6. `apps/server` — native composition root (stub)

`apps/server/Cargo.toml`: package `braincrawl-server`, `[[bin]] name = "braincrawl-server"`. Dependencies: `core`, plus the local adapters `braincrawl-blob-fs`, `braincrawl-store-sqlite`, `braincrawl-store-mem`, `braincrawl-resolver-kv` (path deps). `src/main.rs`: a plain `fn main()` that prints a startup banner and a comment block describing that it wires the local adapters and serves core use-cases over HTTP in a later task. Reference at least the adapter struct names in a comment so the intent is clear; keep `main` compiling with no async runtime.

### 7. `apps/worker` — Cloudflare composition root (stub)

`apps/worker/Cargo.toml`: package `braincrawl-worker`, `[lib] crate-type = ["cdylib", "rlib"]`. Dependencies: `core`, plus `braincrawl-blob-r2`, `braincrawl-store-d1`, `braincrawl-resolver-kv` (path deps). `src/lib.rs`: `#![allow(dead_code)]`, a module doc stating this is the Wasm entry point that binds R2 + D1 + KV and routes into core use-cases (wired with `workers-rs` in a later task). No `workers-rs` dependency. A placeholder `pub fn entrypoint_placeholder() {}` is fine.

Also create `apps/worker/wrangler.toml` as a placeholder declaring the intended bindings in comments (R2 bucket, D1 database, KV namespace, Durable Object, Queue) per `doc02.02.00` — comments only, no live ids.

### 8. Format

Run `cargo fmt --all` after writing everything so the verification gate passes.

## Files to Modify

- `Cargo.toml` — workspace root
- `.gitignore` — `/target`
- `crates/core/{Cargo.toml,src/lib.rs,src/types.rs,src/ports.rs}`
- `crates/sql/{Cargo.toml,src/lib.rs}`
- `crates/adapters/{blob-r2,blob-fs,blob-mem,store-d1,store-sqlite,store-mem,resolver-kv}/{Cargo.toml,src/lib.rs}`
- `apps/server/{Cargo.toml,src/main.rs}`
- `apps/worker/{Cargo.toml,src/lib.rs,wrangler.toml}`
- `migrations/0001_init.sql`

## Verification

```bash
cargo check --workspace
cargo fmt --all -- --check
for d in crates/core crates/sql \
         crates/adapters/blob-r2 crates/adapters/blob-fs crates/adapters/blob-mem \
         crates/adapters/store-d1 crates/adapters/store-sqlite crates/adapters/store-mem \
         crates/adapters/resolver-kv apps/worker apps/server migrations; do
  test -d "$d" || { echo "MISSING DIR: $d"; exit 1; }
done
test -f apps/worker/wrangler.toml || { echo "MISSING wrangler.toml"; exit 1; }
test -f migrations/0001_init.sql || { echo "MISSING migration"; exit 1; }
# core must not depend on any infrastructure crate
! grep -Eiq 'workers-rs|^worker|sqlx|rusqlite|tokio|cloudflare' crates/core/Cargo.toml || { echo "core has infra deps"; exit 1; }
echo "OK"
```

## Out of Scope

- Real adapter implementations (R2/D1/KV/SQLite/filesystem bodies) — later tasks.
- HTTP routing, the `workers-rs` fetch handler, and core use-case functions (`store_fulltext`, read path, `resolve_id`).
- A migration runner, query strings in `crates/sql`, and any test harness beyond what `cargo check` covers.

## Notes

- The `?Send` bound on every port trait is intentional (`doc02.04`): Worker futures are
  `!Send`. Keep one trait definition for both runtimes.
- `todo!()` bodies type-check because `!` coerces to any return type, so `cargo check`
  stays green with no infra deps pulled in.
- Watch for `cargo fmt --check` failures — run `cargo fmt --all` before finishing.

## Surface after this phase

- A buildable Cargo workspace at repo root; `cargo check --workspace` passes.
- Crate `braincrawl-core` exporting `types` (`WorkId`, `Kind`, `Rights`, `StoredBlob`,
  `PayloadDescriptor`, `DomainError`) and `ports` (`BlobStore`, `PayloadsRepo`,
  `MetadataStore`, `IdResolver` — all `#[async_trait(?Send)]`).
- Adapter crates `braincrawl-{blob-r2,blob-fs,blob-mem,store-d1,store-sqlite,store-mem,resolver-kv}`,
  each a struct `impl`ing its port with `todo!()` bodies and depending only on `core` +
  `async-trait`.
- `braincrawl-sql` crate (empty home for shared queries), `apps/server` (native bin stub
  wiring local adapters), `apps/worker` (`cdylib`+`rlib` stub wiring Cloudflare adapters,
  with `wrangler.toml`).
- `migrations/0001_init.sql` creating the `payloads` table.
- Negative space: no infrastructure dependencies anywhere yet; all adapter bodies are
  `todo!()`; no use-case logic, routing, or fetch handler exists.
