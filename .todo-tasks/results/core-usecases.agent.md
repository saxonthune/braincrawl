# Agent Result: core-usecases

date: 2026-06-16T14:43:46-04:00
session: completed
verification: passed
commits: 4
branch: chain-impl-engine_claude_core-usecases
surface deviations: none
session id: 262b856e-e28a-4f05-ae32-0cd45b45c555


## Summary

4 commits. All 7 tests pass. Full workspace builds clean.

---

## Implementation Summary

Phase 2 is complete. The `impl-engine` chain now has a fully exercised in-memory use-case stack.

**What was implemented:**

- **`crates/backends/resolver-mem`** — new crate; `MemResolver` implements `IdResolver` via `RefCell<HashMap>`.
- **`crates/backends/coord-local`** — new crate; `LocalCoordinator` (no-op `Coordinator`), `SystemClock` (std wall-clock → RFC 3339 with no external deps), `UuidGen` (uuid v4). Clock and IdGen live here since they share the same no-infra profile.
- **`crates/backends/store-mem`** — full implementation replacing all `todo!()`s. `MemStore` implements both `MetadataStore` and `PayloadsRepo` via `RefCell<MemStoreInner>`. Full union-find: `resolve_live` with path compression; `merge` with alias repointing, node-assertion folding (newer `fetched_at` wins per source), edge PK-collision folding, payload repointing. Stable-sorted `read_edges` with decimal-offset opaque cursor.
- **`crates/backends/blob-mem`** — full implementation. `MemBlobStore` via `RefCell<HashMap>`. Content hashing uses FNV-1a 64-bit (no extra deps, deterministic, stable; formatted `"fnv1a:<16-hex>"`).
- **`crates/core/src/types.rs`** — `#[derive(Clone)]` on `StoredBlob` and `PayloadDescriptor`; new `ContentOutcome` enum.
- **`crates/core/src/usecases.rs`** — new; `Store<M,B,P,R,C,Clk,Id>` with `put_work`, `get_work`, `put_content`, `get_content`, `put_edges`, `get_edges`, `have`.
- **`crates/core/tests/engine.rs`** — 7 integration tests, all passing.

## Notes

- `Store.resolver` (`R: IdResolver`) is present in the type signature but not used in Phase 2 use-case logic. It's wired for Phase 5 (Cloudflare KV cache). The field is `pub` so callers can wire it at construction time.
- Stub nodes minted by `put_edges` use `NodeKind::Work` as the default kind since edge inputs carry no kind information. A later phase could infer kind from alias namespace (e.g., ORCID → Author).
- `upsert_node_assertion` always overwrites for the given `(id, source)` pair. The "newer wins" merge logic applies only during `merge()` (fold across losers) and at read time in `get_work` (across sources).
- `put_content` with `LinkOnly` rights skips blob storage and stores only the descriptor; `get_content` returns `ContentOutcome::RedirectUrl(source_url)` for these.
- The FNV-1a hash function is duplicated between `blob-mem` (`pub fn fnv1a_hash`) and `usecases.rs` (private `fn fnv1a_hash`). Both use identical constants so hashes match. A shared utility crate would be cleaner but adds a dependency that the plan didn't call for.
- Real-infra backends (`store-sqlite`, `store-d1`, `blob-r2`, `resolver-kv`) and both app entry points remain `todo!()`/stubs, unchanged.

## Surface Deviations

None.

## Commits

```
a875884 feat: core types updates + usecases layer (Store + ContentOutcome)
f168b51 feat: implement store-mem and blob-mem in-memory backends
a5f6157 feat: add resolver-mem and coord-local in-memory backends
d4305db feat: add resolver-mem and coord-local to workspace; add uuid dep
```

## Build & Test Output (last 30 lines)

```

   Doc-tests braincrawl_sql

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests braincrawl_store_d1

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests braincrawl_store_mem

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests braincrawl_store_sqlite

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests braincrawl_worker

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```
