# Core Domain Model

Phase 1 of 5 — the `impl-engine` chain. Expands `crates/core` to the full data
model behind the SQL schema (`migrations/0001_init.sql`, `0002_graph.sql`) and the
store API (`.rhidoc/02-architecture/01-core/03-openapi.yaml`, doc02.01.03). This
phase defines types and traits only — no behavior, no infrastructure. Backends stay
`todo!()`; this phase only updates their signatures to match the new traits so the
workspace keeps compiling.

## Do NOT

- Do NOT implement any use-case logic (resolution, merge, rights gating). That is Phase 2.
- Do NOT touch any backend body — leave every `todo!()` in place. Only adjust
  `use` imports and method signatures so the stubs still satisfy the traits.
- Do NOT add infrastructure deps (rusqlite, workers-rs, axum, uuid) anywhere. Core
  stays platform-agnostic (doc02.04). The only new dep is `serde`/`serde_json`.
- Do NOT add a concrete GUID generator or clock — those are *traits* here; their
  impls come in later phases.
- Do NOT rename crates or move files.

## Plan

### 1. Resolve the `Kind` clash and add id types (`crates/core/src/types.rs`)

The SQL has two distinct "kind" concepts; the code currently conflates them.

- Rename the existing `Kind { Abstract, Fulltext }` to **`PayloadKind`** (it describes
  a blob — `payloads.kind`).
- Add **`NodeKind { Work, Author, Venue, Concept, Topic }`** (matches `node.kind`).
- Rename `WorkId(String)` to **`CanonicalId(String)`** — the braincrawl-minted GUID
  (doc02.01.02). Keep it a newtype over `String`.
- Add **`Alias { namespace: String, value: String }`** — an external id reference
  (`alias` table). This is also the inbound id shape the API accepts ("any id in").
- Add `Rights` already exists — keep it; derive `Clone`.
- Derive `Clone`, `Debug`, `PartialEq` on the value types; add `serde::{Serialize, Deserialize}`
  where they cross the API boundary (`Alias`, `NodeKind`, `PayloadKind`, `Rights`,
  `CanonicalId`).

### 2. Add the API-surface DTOs (`types.rs`)

Model the OpenAPI request/response shapes (`03-openapi.yaml`):

- `WorkRecord { source: String, kind: NodeKind, aliases: Vec<Alias>, attrs: serde_json::Value }`
  — a provider record; `aliases` is the id bundle, never stripped (doc02.01.02).
- `WorkView { canonical_id: CanonicalId, kind: NodeKind, attrs: serde_json::Value,
  provenance: serde_json::Value, aliases: Vec<Alias> }` — the merged read view.
- `EdgeInput { src: Alias, dst: Alias, relation: String, source: String,
  attrs: Option<serde_json::Value>, fetched_at: String }` — one provider edge assertion.
- `EdgeView { src: CanonicalId, dst: CanonicalId, relation: String,
  assertions: Vec<serde_json::Value> }` — merged read view of one edge.
- `EdgeDir { Forward, Backward }`.
- Extend `PayloadDescriptor` to carry every column the use-cases will need to write a
  `payloads` row: `canonical_id`, `kind: PayloadKind`, `version`, `r2_key`,
  `content_hash`, `byte_size`, `mime`, `rights`, `source: Option<String>`,
  `source_url: Option<String>`, `fetched_at: String`, `is_current: bool`.

### 3. Expand the trait surface (`crates/core/src/traits.rs`)

Define the full set of methods the Phase-2 use-cases will call. Keep `#[async_trait(?Send)]`
(doc02.04 — no `Send` bound).

- `BlobStore` — keep `put`/`get`/`delete`.
- `MetadataStore` — replace the thin `upsert_stub`/`edges_out` with the real surface:
  - `get_alias(&self, alias: &Alias) -> Result<Option<CanonicalId>, DomainError>`
  - `get_or_create_alias(&self, alias: &Alias, candidate: &CanonicalId) -> Result<CanonicalId, DomainError>`
    — get-or-create on the `UNIQUE(namespace,value)` constraint (doc02.01.02 §Concurrency):
    `INSERT … ON CONFLICT DO NOTHING` then `SELECT`; returns the winner's id.
  - `mint_node(&self, id: &CanonicalId, kind: NodeKind, created_at: &str) -> Result<(), DomainError>`
  - `upsert_node_assertion(&self, id: &CanonicalId, source: &str, attrs: &serde_json::Value, fetched_at: &str) -> Result<(), DomainError>`
  - `resolve_live(&self, id: &CanonicalId) -> Result<CanonicalId, DomainError>` — follow the
    `merged_into` chain to the live representative (path-compressed).
  - `merge(&self, survivor: &CanonicalId, loser: &CanonicalId) -> Result<(), DomainError>`
    — repoint alias/node_assertion/edge/edge_assertion/payloads, fold PK collisions, tombstone the loser.
  - `read_node(&self, id: &CanonicalId) -> Result<Option<(NodeKind, Vec<(String, serde_json::Value, String)>, Vec<Alias>)>, DomainError>`
    — returns (kind, [(source, attrs, fetched_at)], aliases) for read-time merge.
  - `put_edge(&self, src: &CanonicalId, dst: &CanonicalId, relation: &str, source: &str, attrs: Option<&serde_json::Value>, fetched_at: &str) -> Result<(), DomainError>`
  - `read_edges(&self, id: &CanonicalId, dir: EdgeDir, cursor: Option<&str>, limit: u32) -> Result<(Vec<EdgeView>, Option<String>), DomainError>`
  - `present_aliases(&self, aliases: &[Alias]) -> Result<Vec<Alias>, DomainError>` — for `have`.
- `PayloadsRepo` — keep `current_payload`/`next_version`/`record`; update to `PayloadKind`/`CanonicalId`.
- `IdResolver` — the KV projection cache: `resolve(namespace, value) -> Option<CanonicalId>`,
  `remember(canonical, namespace, value)`. Update to `CanonicalId`.
- Add **`Clock`** trait: `fn now_rfc3339(&self) -> String;` (sync — no infra).
- Add **`IdGen`** trait: `fn new_guid(&self) -> CanonicalId;`.
- Add **`Coordinator`** trait: `async fn with_lock(&self, key: &str) -> Result<(), DomainError>;`
  plus a release, OR a guard-returning shape — keep it minimal and documented as the
  per-work single-writer seam (doc02.01.02 §Concurrency, doc02.02.00). A no-op impl
  must be trivially writable.
- Extend `DomainError` with variants the use-cases need: `Conflict`, `Backend(String)`,
  `Serde(String)`. Keep `NotFound`, `RightsViolation`.

### 4. Keep backends compiling

For each crate under `crates/backends/`, update its `use braincrawl_core::{traits::…, types::…}`
imports and every method signature to match the new traits, leaving the body `todo!()`.
`store-mem`/`store-d1`/`store-sqlite` must now declare (still-`todo!()`) impls for the
*expanded* `MetadataStore`. Do not implement logic.

### 5. Cargo wiring

Add to `[workspace.dependencies]` in root `Cargo.toml`:
`serde = { version = "1", features = ["derive"] }` and `serde_json = "1"`. Add both to
`crates/core/Cargo.toml`. No other manifest changes.

## Files to Modify

- `crates/core/src/types.rs` — the rename + new DTOs
- `crates/core/src/traits.rs` — full trait surface + Clock/IdGen/Coordinator
- `crates/core/Cargo.toml`, root `Cargo.toml` — serde deps
- `crates/backends/*/src/lib.rs` (all 7) — signature/import updates only, bodies stay `todo!()`

## Verification

```bash
cargo build --workspace
cargo test --workspace
```

## Out of Scope

- All use-case behavior (Phase 2).
- Fuzzy record-linkage fallback and Layer 3 collections — no schema/API yet; not in this chain.

## Surface after this phase

- `crates/core` exports, in `types`: `CanonicalId`, `Alias`, `NodeKind`, `PayloadKind`,
  `Rights`, `StoredBlob`, `PayloadDescriptor` (full column set), `WorkRecord`, `WorkView`,
  `EdgeInput`, `EdgeView`, `EdgeDir`, `DomainError` (with `Conflict`/`Backend`/`Serde`/`NotFound`/`RightsViolation`).
- In `traits`: `BlobStore`, `PayloadsRepo`, `MetadataStore` (full surface listed in Plan §3),
  `IdResolver`, `Clock`, `IdGen`, `Coordinator`, all `#[async_trait(?Send)]` (sync for Clock/IdGen).
- `WorkId` no longer exists — it is `CanonicalId`. `Kind` no longer exists — payloads use
  `PayloadKind`, nodes use `NodeKind`.
- All 7 backend crates compile against the new traits with `todo!()` bodies.
- `cargo build --workspace` and `cargo test --workspace` pass. No infra deps in core.
