# Agent Result: shared-sql

date: 2026-06-16T14:51:02-04:00
session: completed
verification: passed
commits: 1
branch: chain-impl-engine_claude_shared-sql
surface deviations: none
session id: a2cb7252-85ee-4b29-85e3-dcb061164700


## Summary

Commit confirmed. The implementation is complete and all tests pass.

## Implementation summary

**Files changed:**
- `crates/sql/src/lib.rs` — replaced the placeholder with a full query catalogue organized into submodules (`alias`, `node`, `node_assertion`, `edge`, `edge_assertion`, `payload`), migration consts (`MIGRATION_0001`/`MIGRATION_0002` via `include_str!`) + `migrations()` fn, and `in_list`/`alias_pair_list` builder functions.
- `crates/sql/Cargo.toml` — added `rusqlite = { version = "0.31", features = ["bundled"] }` as dev-dependency.
- `crates/sql/tests/sql_valid.rs` — new integration test: applies migrations to an in-memory SQLite DB, prepares all exported query strings (with 1- and 3-element builder variants), and runs three test cases: all-prepare check, happy-path execution, and a merge smoke test.

## Notes

- The edge merge query ordering documented in the module-level doc comment handles the subtle `loser→loser` self-edge case correctly: src repoint first creates `survivor→loser`, then dst repoint converts that to `survivor→survivor`, which is properly deduplicated by `ON CONFLICT DO NOTHING`.
- FK enforcement on `edge_assertion → edge` creates a transient violation during merge (assertions are upserted before the edge row exists under the new src). The doc comment calls this out and recommends deferred FK checking or disabling FK enforcement for the merge transaction.
- `node_assertion::MERGE_UPSERT` uses a conditional DO UPDATE (`WHERE excluded.fetched_at > node_assertion.fetched_at`) to keep the newer assertion on conflict — matching the in-memory backend's semantics exactly.
- `payload::MERGE_REPOINT` uses `ON CONFLICT DO NOTHING` (not DO UPDATE) for the PK `(canonical_id, kind, version)` — version numbers are minted independently per node so collisions are extremely unlikely, but if they occur the survivor's version is silently preferred.

## Surface Deviations

None.

## Commits

```
7591eb8 feat: sql query catalogue + validation test (Phase 3)
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
