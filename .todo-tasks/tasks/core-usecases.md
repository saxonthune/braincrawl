# Core Use-Cases + In-Memory Engine

Phase 2 of 5 — the `impl-engine` chain. Implements the platform-agnostic use-case
layer (the store API of doc02.01.03 / `03-openapi.yaml`) and the in-memory backends
needed to unit-test it end to end. After this phase the engine is fully exercised in
memory; later phases only swap in real storage.

Triage against Phase 1's **Surface after this phase** (core-domain-model): the types
and traits listed there exist; assume nothing beyond them.

## Do NOT

- Do NOT touch real-infra backends (`store-sqlite`, `store-d1`, `blob-r2`, `resolver-kv`)
  or the entry points (`apps/server`, `apps/worker`). Leave them `todo!()`.
- Do NOT add infra deps (rusqlite, workers-rs, axum). The in-memory backends use only
  `std` collections + `serde_json`. The `IdGen` impl may use `uuid`.
- Do NOT implement the fuzzy record-linkage fallback (doc02.01.02 §Fuzzy). Resolution
  here is union-find over the id bundle only.
- Do NOT auto-merge on a single shared id without it being witnessed by a record's bundle.

## Plan

### 1. Use-case module (`crates/core/src/usecases.rs`, add `pub mod usecases;` to `lib.rs`)

Implement free functions / a `Store<M,B,P,R,C,Clk,Id>` struct generic over the traits.
Each maps to one API operation (doc02.01.03):

- `put_work(record: WorkRecord) -> CanonicalId` — the union-find resolve (doc02.01.02
  §"resolve(record)→guid"):
  1. Take `record.aliases` as the bundle.
  2. Look each up via `MetadataStore::get_alias`; collect distinct live GUIDs `G`
     (run each through `resolve_live`).
  3. `|G|==0` → `IdGen::new_guid`, `mint_node`, insert all aliases via `get_or_create_alias`.
  4. `|G|==1` → use it; insert any missing aliases.
  5. `|G|>=2` → pick deterministic survivor (lexicographically smallest GUID), `merge`
     each loser into it, then attach aliases.
  Then `upsert_node_assertion(guid, record.source, &record.attrs, clock.now)`. Wrap the
  whole resolve+merge in `Coordinator::with_lock` keyed by the strongest id in the bundle
  (priority `doi > pmid > openalex > …`; document the ordering).
- `get_work(id: Alias) -> Option<WorkView>` — resolve id → live GUID → `read_node` →
  merge assertions into one `attrs` object with per-field `provenance` (winning source =
  newest `fetched_at`; document the tie-break). Returns None if unknown.
- `put_content(id, kind, body, rights, source, source_url, fetched_at)` — resolve id;
  if `rights == Restricted`, never persist bytes (return a descriptor only / error per
  the 451 semantics); else `next_version`, compute `content_hash` + `byte_size`, build
  `r2_key = {canonical}/{kind}/v{version}`, `BlobStore::put` (open) and `PayloadsRepo::record`.
- `get_content(id, kind) -> ContentOutcome` — resolve; read current payload; map to the
  rights-gated outcomes (bytes | redirect url | pending | restricted | absent — mirror
  the 200/302/202/451/404 cases as an enum).
- `put_edges(edges: Vec<EdgeInput>) -> usize` — for each: resolve src/dst (minting stub
  nodes via `get_or_create_alias` + `mint_node` when unknown — a stub is a node with no
  assertions), then `put_edge` (deduped on `(src,dst,relation)`; the provider claim is an
  assertion). Return count written.
- `get_edges(id, dir, cursor, limit) -> (Vec<EdgeView>, Option<String>)` — resolve, delegate
  to `read_edges`.
- `have(aliases) -> Vec<Alias>` — delegate to `present_aliases`.

Add a `ContentOutcome` enum to `types.rs` if needed for `get_content`.

### 2. In-memory backends

Implement these so use-cases are testable without infra:

- `crates/backends/store-mem` — back `MetadataStore` + `PayloadsRepo` with `RefCell<HashMap>`s
  mirroring the SQL tables (`node`, `alias`, `node_assertion`, `edge`, `edge_assertion`,
  `payloads`). Implement the real semantics: alias uniqueness/get-or-create, `merged_into`
  chain + path compression in `resolve_live`, the full `merge` repoint with PK-collision
  folding, edge dedup, pagination over a stable sort for `read_edges`.
- `crates/backends/blob-mem` — `HashMap<String, (Vec<u8>, mime)>`; compute a `content_hash`
  (any stable hash, e.g. SHA-256 via a tiny dep or a simple FNV — document choice).
- Create `crates/backends/resolver-mem` — in-memory `IdResolver` (HashMap projection).
  Add to workspace members + a `Cargo.toml`.
- Create `crates/backends/coord-local` — `Coordinator` no-op (single-threaded, so the
  alias UNIQUE constraint is the real serialization point; the lock is a no-op here).
  Add to workspace members + a `Cargo.toml`. This same trait is bound to a Durable Object
  in Phase 5.
- Provide simple `Clock`/`IdGen` impls usable by tests — a `SystemClock`/`UuidGen` can
  live in `coord-local` or a small `crates/backends/host-util` crate (document where).
  `UuidGen` may depend on `uuid`.

### 3. Tests (in `crates/core/tests/` or `#[cfg(test)]` in `usecases.rs`)

Cover the doc02.01.02 guarantees against the in-memory stack:

- Idempotent `put_work` — same record twice → one node.
- Bundle convergence — two records sharing one id but each adding a new id converge onto
  one node regardless of arrival order.
- Merge confluence — a bridging record carrying two previously-separate ids merges them;
  deterministic survivor; old GUID resolves through the tombstone.
- Stub creation — `put_edges` with an unknown `dst` creates a stub node.
- Edge dedup + multi-source assertion retention.
- `have` returns only present ids.
- `put_content`/`get_content` rights gating (open persists; restricted does not).

## Files to Modify

- `crates/core/src/usecases.rs` (new), `crates/core/src/lib.rs`, `crates/core/src/types.rs`
- `crates/backends/store-mem/src/lib.rs`, `crates/backends/blob-mem/src/lib.rs`
- `crates/backends/resolver-mem/` (new crate), `crates/backends/coord-local/` (new crate)
- root `Cargo.toml` (new members + `uuid` workspace dep)
- `crates/core/tests/engine.rs` (new) or inline tests

## Verification

```bash
cargo build --workspace
cargo test --workspace
```

## Out of Scope

- SQLite/D1/R2/KV backends, HTTP server, worker (Phases 3-5).
- Fuzzy fallback, Layer 3.

## Surface after this phase

- `braincrawl_core::usecases` exposes `put_work`, `get_work`, `put_content`, `get_content`,
  `put_edges`, `get_edges`, `have` (as a generic `Store` over the Phase-1 traits, or free
  functions taking trait objects — document the exact signatures used).
- `types::ContentOutcome` exists for `get_content`.
- `store-mem` + `blob-mem` fully implement their traits with correct union-find/merge/dedup
  semantics; `resolver-mem` and `coord-local` crates exist and implement `IdResolver` and a
  no-op `Coordinator`; a host `Clock`/`IdGen` impl exists (location documented).
- The doc02.01.02 guarantees (idempotency, convergence, confluent merge, stub creation) are
  covered by passing tests. `cargo test --workspace` is green.
- Real-infra backends (`store-sqlite`, `store-d1`, `blob-r2`, `resolver-kv`) and both entry
  points remain `todo!()`/stubs, unchanged.
