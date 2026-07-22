# Artifact listing, part 1 — the store seam

## Motivation

A work can hold many artifacts: one per role (`fulltext`, `text`, `chunks`,
`abstract`, or any `[a-z0-9_-]+` slug), and a monotonic version series within
each role. The schema records all of it — `artifacts(canonical_id, role, version,
r2_key, content_hash, byte_size, mime, source, source_url, fetched_at, is_current)`
with the blob key `{canonical_id}/{role}/v{version}` — and none of it is
readable back.

`ArtifactStore` exposes only `current_artifact`, `next_version`, and `record`.
There is no way to ask what roles a work holds, so an artifact is reachable only
by someone who already knows its role name, and superseded versions are written
and then permanently unreachable. `WorkView` carries attrs, provenance, and
aliases but no artifact list.

This phase adds the read at the store seam: a trait method, a use-case method,
the SQL, and all three backends. The HTTP surface and the CLI verb follow in the
next phase.

## Do NOT

- Do NOT add an HTTP route, touch `apps/server`, `apps/worker`, `crates/conformance`,
  `apps/cli`, or `StoreClient`. Those are the next phase. This phase stops at the
  store seam.
- Do NOT add a field to `WorkView` or change `get_work`. Artifact listing is its
  own read, not a widening of the work read — a work with many large artifact
  series should not make every metadata read heavier.
- Do NOT write a schema migration. Every column this phase reads already exists;
  `migrations/0001_init.sql` and `0004_rename_payloads_to_artifacts.sql` created
  them, and `artifacts_current` already indexes `(canonical_id, role)`.
- Do NOT change `current_artifact`, `next_version`, or `record`, or any of their
  call sites. This phase is purely additive at the trait.
- Do NOT read blob bytes. This returns descriptors only; the bytes stay behind
  `get_content`.
- Do NOT invent a new descriptor type. Reuse `Artifact` from
  `crates/core/src/types.rs`.

## Plan

### 1. Add the trait method

In `crates/core/src/traits.rs`, the `ArtifactStore` trait (around line 17) gains
one method beside the existing three:

```rust
/// Every artifact descriptor held for a work, newest role/version first.
/// `role` restricts to a single role; `all_versions` includes superseded
/// versions rather than only the current one per role.
async fn list_artifacts(
    &self,
    id: &CanonicalId,
    role: Option<ArtifactRole>,
    all_versions: bool,
) -> Result<Vec<Artifact>, DomainError>;
```

Adding a method to the trait breaks all three implementations until step 4 —
that is expected; the workspace will not build until every backend is done.

### 2. Add the SQL

In `crates/sql/src/lib.rs`, the `artifact` module (around line 298) holds
`SELECT_CURRENT`, `NEXT_VERSION`, `FLIP_CURRENT_OFF`, and the insert. Add the
listing statements beside them, following the existing column order exactly so
row-reading code stays uniform with `SELECT_CURRENT`:

- `LIST_CURRENT` — every current artifact for a canonical id, all roles.
  Params: `(canonical_id)`. Filter `is_current = 1`. Order by `role`, then
  `version DESC`.
- `LIST_ALL_VERSIONS` — every artifact for a canonical id regardless of
  `is_current`. Params: `(canonical_id)`. Same ordering.
- `LIST_CURRENT_BY_ROLE` — as `LIST_CURRENT`, restricted to one role.
  Params: `(canonical_id, role)`.
- `LIST_ALL_VERSIONS_BY_ROLE` — as `LIST_ALL_VERSIONS`, restricted to one role.
  Params: `(canonical_id, role)`.

Four flat statements rather than one built by string concatenation: the crate's
existing convention is complete constant statements, and it keeps the SQL
greppable.

### 3. Add the use-case method

In `crates/core/src/usecases.rs`, beside `put_content` and `get_content`, add:

```rust
pub async fn list_artifacts(
    &self,
    id: Alias,
    role: Option<ArtifactRole>,
    all_versions: bool,
) -> Result<Vec<Artifact>, DomainError>
```

It resolves the alias to a live canonical id using the same helper `put_content`
uses (`resolve_alias_to_live`) and delegates to `self.artifacts.list_artifacts`.
An unknown alias must return the same error `get_content` returns for an unknown
alias — do not invent a new error variant, and do not silently return an empty
list for a work that does not exist. A known work with no artifacts returns an
empty vector.

This is a read, so it must NOT take the coordinator lock.

### 4. Implement in all three backends

- `crates/backends/store-sqlite/src/lib.rs` (impl at line 116) — use
  `conn.prepare` and `query_map` with the statement chosen by the `role` and
  `all_versions` arguments. The existing `current_artifact` in the same impl
  shows the row-to-`Artifact` mapping, including `parse_artifact_role`; factor
  that mapping into a shared private helper rather than writing it twice.
- `crates/backends/store-d1/src/lib.rs` (impl at line 162) — same four
  statements, using the crate's existing D1 query and row-decoding idiom as seen
  in its `current_artifact`.
- `crates/backends/store-mem/src/lib.rs` (impl at line 132) — filter the
  in-memory collection directly. Match the SQL ordering (`role` ascending, then
  `version` descending) so tests written against one backend hold for the other.

### 5. Test

Add unit tests wherever the crate already keeps them for these types. Cover, at
minimum, against `MemStore` (no database required):

- a work with three roles returns three descriptors with `all_versions: false`
- storing the same role twice returns one current descriptor by default and two
  with `all_versions: true`, with `is_current` true on exactly one
- `role: Some(...)` restricts the result to that role
- a known work with no artifacts returns an empty vector
- ordering is stable and matches the documented order

If `crates/backends/store-sqlite` already carries tests against a temporary
database, add the equivalent cases there too, so the SQL is exercised and not
just the in-memory shortcut.

## Files to Modify

- `crates/core/src/traits.rs` — add `list_artifacts` to `ArtifactStore`
- `crates/core/src/usecases.rs` — add `Store::list_artifacts`
- `crates/sql/src/lib.rs` — add four `SELECT` constants to the `artifact` module
- `crates/backends/store-sqlite/src/lib.rs` — implement; extract the shared
  row-to-`Artifact` mapping
- `crates/backends/store-d1/src/lib.rs` — implement
- `crates/backends/store-mem/src/lib.rs` — implement, plus the unit tests

## Verification

```bash
cargo build
cargo test
cargo build -p braincrawl-cli --bin braincrawl
```

`cargo build` must succeed for the whole workspace, which is the real gate — the
trait change breaks every backend that has not been updated.

## Out of Scope

- Any HTTP route, `StoreClient` method, or CLI verb.
- The worker conformance suite.
- Deleting or pruning superseded artifact versions.
- Exposing `r2_key` or `content_hash` to users — they are in the `Artifact`
  struct already and the next phase decides what to render.

## Notes

- The worker build is a separate target (`just worker-test` boots it under
  wrangler). This phase changes `store-d1`, which the worker compiles, so a
  `cargo build` failure there is in scope even though the worker's own routes are
  not.
- `artifacts_current` indexes `(canonical_id, role) WHERE is_current = 1`, so
  `LIST_CURRENT` is served by an index while `LIST_ALL_VERSIONS` is not. That is
  acceptable — listing every version of every role is the rare path.
- Reviewer watch item: the three backends must agree on ordering and on what an
  unknown alias does. A disagreement here surfaces as a conformance failure only
  after the next phase adds the HTTP surface, which is much later and harder to
  diagnose.

## Surface after this phase

- `braincrawl_core::traits::ArtifactStore` has a fourth method:
  `async fn list_artifacts(&self, id: &CanonicalId, role: Option<ArtifactRole>, all_versions: bool) -> Result<Vec<Artifact>, DomainError>`.
  Results are ordered by role ascending, then version descending. With
  `all_versions: false` at most one descriptor per role is returned.
- `braincrawl_core::usecases::Store` has
  `pub async fn list_artifacts(&self, id: Alias, role: Option<ArtifactRole>, all_versions: bool) -> Result<Vec<Artifact>, DomainError>`.
  It resolves any external alias to its live canonical id, takes no coordinator
  lock, returns an empty vector for a known work with no artifacts, and returns
  the same error `get_content` returns for an unknown alias.
- `braincrawl_sql::artifact` exposes `LIST_CURRENT`, `LIST_ALL_VERSIONS`,
  `LIST_CURRENT_BY_ROLE`, and `LIST_ALL_VERSIONS_BY_ROLE`, each selecting the
  same eleven columns in the same order as `SELECT_CURRENT`.
- `SqliteStore`, `D1Store`, and `MemStore` all implement the new method with
  matching semantics.
- Negative space: `Artifact` and `WorkView` in `crates/core/src/types.rs` are
  unchanged — `WorkView` still has no artifact field. `current_artifact`,
  `next_version`, and `record` are unchanged. No migration was added. The store's
  HTTP contract is unchanged: there is still no artifact-listing route, no
  `StoreClient` method, and no `library list` CLI verb. Every CLI verb behaves
  exactly as it did after the previous phase.
