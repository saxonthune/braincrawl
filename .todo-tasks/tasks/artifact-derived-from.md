# Record what an artifact was derived from, so a work's artifacts form a tree

## Motivation

`Artifact` (`crates/core/src/types.rs:66-78`) is keyed by `(canonical_id, role, version)` and
has no field pointing at a parent. `source` is a free label naming the *process* —
`"extract-text"`, `"user-supplied"` — not the input. So a work's artifacts are a flat bag, and
`library list` prints them as one.

That is wrong about the domain. A catalog entry can hold several unrelated **root** artifacts —
the Jung entry holds a reflowed ebook at `fulltext` and a re-OCR'd scan at `fulltext-1964-scan`,
with different pagination — each with its own chain of derivations hanging off it. Today that
structure lives only in a role-slug naming convention (`fulltext-1964-scan` → `text-1964-scan`)
that nothing parses, enforces, or displays. Phases 1 and 2 made the input selectable and made
the derived artifacts record a `source_role` inside their own JSON payload; this phase moves
that fact into the artifact record where the store can answer questions with it.

## Do NOT

- Do **not** infer lineage from role-slug spelling. `fulltext-1964-scan` and `text-1964-scan`
  look related; that resemblance is a convention, not data, and guessing from it would bake the
  convention in permanently.
- Do **not** make `derived_from` required. A root artifact has none, and `library put` of a
  user-supplied file must keep working with no extra flag.
- Do **not** backfill existing rows with a guess. Existing artifacts get `NULL` and are treated
  as roots. Say so in the migration.
- Do **not** enforce referential integrity in the schema. A parent artifact may legitimately be
  superseded or removed later; a dangling pointer is displayed as unknown, not rejected.
- Do **not** change `library list`'s JSON field names or remove any existing field. Add.
- Do **not** redesign the `source` / `source_url` provenance fields. They stay as they are.

## Plan

### 1. Schema

Add migration `migrations/0006_artifact_derived_from.sql` following the shape of the existing
files: two nullable columns on the artifacts table, `derived_from_role TEXT` and
`derived_from_version INTEGER`. Both null means a root. Add the migration to the compiled list
so `/health`'s `migrations_compiled` includes it — `apps/server/src/lib.rs` reports applied
against compiled, and a mismatch is what that endpoint exists to surface.

### 2. Core type and trait

Add to `Artifact` (`crates/core/src/types.rs:66-78`):

```rust
pub derived_from: Option<(ArtifactRole, u32)>,
```

`put_content` in `crates/core/src/usecases.rs:184` gains a `derived_from: Option<(ArtifactRole, u32)>`
parameter and threads it into the descriptor it builds at `:203-215`.

### 3. Backends

Three implementations of `record` / `list_artifacts` / `current_artifact` must round-trip the
two columns: `crates/backends/store-sqlite/src/lib.rs`, `crates/backends/store-mem/src/lib.rs`,
`crates/backends/store-d1/src/lib.rs`. The SQL lives in the `braincrawl-sql` crate
(`braincrawl_sql::artifact::*`) and is shared by sqlite and d1 — update it once there.

`row_to_artifact` in the sqlite backend and the `Row` struct in the d1 backend both need the
new columns.

### 4. HTTP surface

`artifact_json` (`apps/server/src/lib.rs:208-222`) gains `derived_from_role` and
`derived_from_version`. The PUT content route (`apps/server/src/lib.rs:413`) accepts optional
`derived_from_role` / `derived_from_version` query parameters. Mirror both in the worker
(`apps/worker/src/lib.rs`, `handle_put_content` and its `artifact_json`) — the two servers share
`crates/core`, and letting them diverge on the wire is what produced the `library list` 404 that
opened this whole line of work.

### 5. CLI

`store_client.rs`'s `put_content_with_fetched_at` passes the two parameters when present.
`library extract-text`, `library chunk`, `library paginate`, and `library outline` each record
their `--from` role and the version they actually read as `derived_from`.

`library list` renders the artifacts as a tree: roots at the top level, derived artifacts nested
under their parent, in role order. A dangling parent prints the artifact at top level with its
recorded parent named as unknown.

## Files to Modify

- `migrations/0006_artifact_derived_from.sql` — new
- `crates/sql/src/lib.rs` (or the `artifact` module within it) — the shared SQL
- `crates/core/src/types.rs` — `Artifact.derived_from`
- `crates/core/src/usecases.rs` — `put_content` parameter and descriptor
- `crates/backends/store-sqlite/src/lib.rs`, `store-mem/src/lib.rs`, `store-d1/src/lib.rs`
- `apps/server/src/lib.rs` — `artifact_json`, PUT content params, compiled migrations list
- `apps/worker/src/lib.rs` — the same two, kept in step
- `apps/cli/src/store_client.rs`, `apps/cli/src/main.rs` — pass and render
- `apps/server/tests/smoke.rs` — round-trip a derived artifact and assert the tree

## Verification

```bash
cargo test
cargo build --bin braincrawl
```

## Out of Scope

- Deleting artifacts, or what a delete does to a chain. There is no delete verb today; see
  `feedback-no-delete-in-library-or-catalog` in the inbox.
- Re-keying artifacts as `(work, copy, role)`, which would name the physical copy as a
  first-class thing. Considered and deferred as too large.
- Any change to the L3 Research Document grammar.

## Notes

- This phase touches both servers. The worker is deployed at `bcp.saxon.zone` and its running
  build is currently behind trunk — a separate `just deploy-worker` is outstanding from earlier
  work. Do not deploy from this task; just keep the code in step.
- The conformance suite (`crates/conformance`) is the place to assert both backends agree, if
  adding a case there is cheap.

## Surface after this phase

- `Artifact` carries `derived_from: Option<(ArtifactRole, u32)>`; `None` means a root artifact.
- `put_content` takes `derived_from` and persists it; all three store backends round-trip it.
- Migration `0006_artifact_derived_from` exists, is compiled in, and adds two nullable columns.
  Pre-existing rows are `NULL` and read as roots.
- `artifact_json` emits `derived_from_role` and `derived_from_version` on both the server and
  the worker; `PUT /works/*/content/*` accepts them as optional query parameters.
- `library list` prints a tree of roots and their derived artifacts.
- `extract-text`, `chunk`, `paginate`, and `outline` record the role and version they read.
- Unchanged: role slugs are still free-form and still the primary key component; `source` and
  `source_url` keep their present meaning; no delete verb exists.
