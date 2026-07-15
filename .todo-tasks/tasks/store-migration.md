# CLI: migrate-store — replay the local corpus into a remote store

## Motivation

The deployed worker's Library+Catalog is empty; the accumulated corpus
lives in the desktop SQLite + blob dir. There is no enumeration surface
over HTTP (confirmed: no list-all route, no trait method), so migration
reads the local database directly and replays through the remote's
public HTTP API — which is idempotent by construction because identity
is alias-derived.

## Do NOT

- Do NOT write to D1/R2 directly — replay through the HTTP surface only
  (`PUT /works`, `PUT /edges`, `PUT .../content/{role}`), so the remote's
  own identity resolution does the work.
- Do NOT add enumeration methods to the `MetadataStore`/`BlobStore`
  traits — the enumeration is migration-specific SQL, kept in the
  migration path.
- Do NOT migrate tombstones (`merged_into IS NOT NULL`) — replaying live
  nodes with their full alias sets reproduces the merge outcomes.
- Do NOT re-upload content the remote already has (version churn):
  check first, skip on present.
- Do NOT parallelize aggressively — sequential with modest concurrency
  (≤4 in flight) is fine; the worker is on a free plan.

## Plan

### 1. Enumeration (direct SQLite)

New module in `apps/cli` (e.g. `src/migrate.rs`) opening the local DB
read-only. Check how `crates/backends/store-sqlite` opens/locates the DB
(path convention: the shared server's `BRAINCRAWL_DB` under
`~/.local/share/braincrawl/`) and reuse its connection dependency
(rusqlite) — add it to `apps/cli` Cargo.toml if not already transitive.

Queries (schema per `migrations/0001,0002,0004,0005`):
- Live nodes: `SELECT canonical_id, kind FROM node WHERE merged_into IS NULL`
- Aliases per node: `SELECT namespace, value FROM alias WHERE canonical_id = ?`
- Assertions per node: `SELECT source, attrs, fetched_at FROM node_assertion WHERE canonical_id = ?`
- Edge assertions: `SELECT src_id, dst_id, relation, source, attrs FROM edge_assertion`
- Current artifacts: `SELECT canonical_id, role, r2_key, mime, source, source_url, fetched_at FROM artifacts WHERE is_current = 1`
  (0004 renamed payloads→artifacts, kind→role; 0005 dropped rights —
  verify final column names against the migrations before writing SQL)

### 2. Replay

`braincrawl migrate-store [--db <path>] [--blobs <path>] [--dry-run]`
(clap verb in `apps/cli/src/cli.rs`; target = the configured
`server_url` + `auth_token`, same plumbing as every other verb):

1. **Works**: for each live node, build one `WorkRecord` per assertion
   (`source`, `kind`, full alias list, `attrs` parsed from the stored
   JSON) and `PUT /works` via the existing `store_client`. A node with
   zero aliases cannot be addressed by the replay — count and report as
   skipped. Nodes with zero assertions but aliases: send one WorkRecord
   with empty attrs so the alias registers (mirror how stubs enter via
   the normal path — check `store_client`/usecase expectations first).
   Prefilter with `POST /works/have` in batches; skip nodes whose every
   alias is already present.
2. **Edges**: for each edge_assertion, resolve src/dst to one alias each
   (prefer `openalex`, then `doi`, else any; if an endpoint has no alias
   → count skipped). Batch `PUT /edges` with `EdgeInput` exactly as the
   CLI's existing edge push does (reuse its construction).
3. **Artifacts**: for each current artifact row, `GET` the remote
   `works/{alias}/content/{role}` — on 200 skip; else read the local
   blob bytes (map `r2_key` to a path per `crates/backends/blob-fs`'s
   key→path scheme — read that crate, don't guess) and
   `PUT .../content/{role}?mime=...&source=...&source_url=...&fetched_at=...`.
4. Report: counts per category (works pushed/skipped, edges, artifacts
   pushed/skipped/missing-blob, no-alias skips), then fetch remote
   `GET /stats` and print alongside local counts. `--dry-run` prints the
   plan counts without any PUT.

### 3. Tests

Unit-test the row→WorkRecord/EdgeInput mapping and the alias-preference
choice with an in-memory SQLite fixture seeded via the real migration
files (rusqlite can execute them). HTTP replay is exercised manually
(document the wrangler-dev smoke in the result notes).

## Files to Modify

- `apps/cli/src/migrate.rs` — new
- `apps/cli/src/cli.rs` — verb
- `apps/cli/Cargo.toml` — rusqlite (read-only use)
- `apps/cli/src/store_client.rs` — only if a needed request helper is
  missing

## Verification

```bash
cargo test -p braincrawl-cli
cargo build --workspace
```

## Out of Scope

- Ongoing two-way store sync (push-on-query converges new material once
  the CLI points at the worker)
- L3 docs (l3 push covers them)
- Jobs table (transient queue state — not migrated)

## Notes

- 50 MB max content size is enforced elsewhere; if a local blob exceeds
  the worker's request tolerance the PUT fails — count it, continue,
  report at the end.
- Run order for the user afterwards: `braincrawl migrate-store`, verify
  stats match, then flip `server_url` in config.toml to the worker.

## Surface after this phase

- `braincrawl migrate-store [--db] [--blobs] [--dry-run]` exists,
  idempotent, with a per-category count report and stats comparison.
- Negative space: no trait changes, no server/worker changes, no D1/R2
  direct writes.
