# Shared SQL Query Strings

Phase 3 of 5 — the `impl-engine` chain. Fills `crates/sql` with the parameterized,
SQLite-dialect query strings that both the local SQLite backend (Phase 4) and the
Cloudflare D1 backend (Phase 5) execute. D1 speaks the SQLite dialect, so one set of
queries serves both; the backends differ only in driver (doc02.04). This phase writes
and validates SQL — it wires nothing into a runtime.

Triage against the Surface of Phase 2 (core-usecases): the `MetadataStore`/`PayloadsRepo`
method set is fixed and the in-memory backend already shows the exact semantics each
query must reproduce. Mirror those semantics in SQL.

## Do NOT

- Do NOT add a runtime DB driver dependency to `crates/sql`'s normal deps. `rusqlite` is a
  **dev-dependency** only, used to validate the SQL against the schema in a test.
- Do NOT implement any `MetadataStore`/`PayloadsRepo` trait — that is Phases 4/5. This crate
  exports query *strings* (and optionally tiny row-mapping helpers), not trait impls.
- Do NOT use any SQLite feature D1 lacks. Stick to plain prepared statements with `?` /
  numbered params. No triggers, no `RETURNING`-only logic that D1 can't run (verify against
  current D1 SQLite support; prefer `INSERT … ON CONFLICT DO NOTHING` + `SELECT`).
- Do NOT change the migrations — `0001_init.sql` / `0002_graph.sql` are the schema of record.

## Plan

### 1. Query catalogue (`crates/sql/src/lib.rs`)

Replace the placeholder `MIGRATIONS` const with named `pub const` query strings (or a
small module per table) covering every operation the Phase-1 `MetadataStore`/`PayloadsRepo`
surface needs. At minimum:

- **alias**: get-or-create (`INSERT INTO alias … ON CONFLICT(namespace,value) DO NOTHING`
  then `SELECT canonical_id FROM alias WHERE namespace=? AND value=?`); lookup by alias;
  repoint aliases from loser→survivor (for merge); list aliases by canonical_id.
- **node**: insert (mint); set `merged_into` (tombstone); select `merged_into` for chain
  walking (`resolve_live` is iterative in the backend, but provide the single-hop SELECT);
  select kind by id.
- **node_assertion**: upsert (`ON CONFLICT(canonical_id,source) DO UPDATE` keeping newest
  `fetched_at`); repoint loser→survivor folding same-source collisions (keep newest);
  select all assertions for a node.
- **edge**: insert-or-ignore (dedup on PK `(src_id,dst_id,relation)`); repoint endpoints
  loser→survivor collapsing duplicates; forward/backward paginated selects using
  `edge_forward`/`edge_backward` indexes with a stable cursor (e.g. keyset on `dst_id`/`src_id`).
- **edge_assertion**: upsert per `(src,dst,relation,source)`; repoint; select by edge.
- **payloads**: select current (`is_current=1`); compute next version
  (`SELECT COALESCE(MAX(version),0)+1 …`); insert new version + flip prior `is_current=0`;
  repoint canonical_id loser→survivor.
- **have**: `SELECT namespace,value FROM alias WHERE (namespace,value) IN (…)` — document
  how the variable-length IN-list is parameterized (the backend expands placeholders).

Group logically and document each const with the operation it backs. Where a statement
needs a dynamic placeholder count (IN-lists, batch inserts), export a small builder fn
`fn in_list(n: usize) -> String` rather than baking a fixed arity.

### 2. Migration loading helper

Export the migration SQL so backends can apply it: either `pub const MIGRATION_0001` /
`MIGRATION_0002` via `include_str!("../../../migrations/000X_*.sql")`, or a
`pub fn migrations() -> &'static [(&'static str, &'static str)]` returning ordered
(name, sql) pairs. Backends run these at startup (local) / via wrangler (cloudflare).

### 3. Validation test (`crates/sql/tests/sql_valid.rs`)

Add `rusqlite` as a dev-dependency. The test:
1. Opens an in-memory SQLite db, applies both migrations.
2. `prepare()`s every exported query string (with representative param counts for builders)
   and asserts each prepares without error — this catches dialect/typo/column errors against
   the real schema.
3. Runs a tiny happy-path sequence (insert alias, mint node, insert edge, select) to confirm
   the statements execute, not just prepare.

## Files to Modify

- `crates/sql/src/lib.rs` — the query catalogue + migration loader
- `crates/sql/Cargo.toml` — `rusqlite` dev-dependency (bundled feature)
- `crates/sql/tests/sql_valid.rs` (new)

## Verification

```bash
cargo build --workspace
cargo test -p braincrawl-sql
cargo test --workspace
```

## Out of Scope

- Trait implementations / driver binding (Phases 4-5).
- Layer 3 collection tables, fuzzy-match blocking keys — not in this chain.

## Surface after this phase

- `braincrawl_sql` exports a named query string (`pub const` or module) for every
  `MetadataStore`/`PayloadsRepo` operation in the Phase-1 trait surface, plus an `in_list`
  (or equivalent) builder for variable-arity statements.
- `braincrawl_sql` exports the ordered migrations (const(s) or `migrations()` fn) for
  startup application.
- A passing `cargo test -p braincrawl-sql` proves every query prepares and the happy path
  executes against the actual schema in in-memory SQLite.
- All strings are D1-compatible SQLite dialect. `rusqlite` is dev-only; the crate adds no
  runtime driver dep.
