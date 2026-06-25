# Provider refactor — Phase 1: the Emission output contract

## Motivation

Adding a third provider (arXiv) would mean a third copy of the per-provider push
plumbing. Today every provider verb returns a provider-private `PushBatch`
(`{records: Vec<(Entity, Value)>, edges}`), and `main.rs` carries three
near-identical `push_*_batch_to_store` + `report_*_push_summary` pairs that differ
only in which provider's `to_work_record`/`to_edges` they call. The store side is
already provider-agnostic — it takes neutral JSON (`{source, kind, aliases, attrs}`
work records and `{src, dst, relation, source, attrs, fetched_at}` edges). The
duplication *is* an un-extracted contract.

This phase extracts that contract as a **serializable `Emission`** and collapses the
three push paths into one. The serializable wire types are deliberate: a future
subprocess plugin will emit exactly this `Emission` JSON on stdout, so the type we
define here becomes the plugin output protocol for free. This phase is the OUTPUT
half of that protocol; Phase 2 adds the INPUT half (a `ProviderCmd` enum + the
`Provider` trait).

This is a pure consolidation refactor — **no observable behavior change**. The CLI's
output, push results, and exit codes must be byte-for-byte identical before and after.

## Do NOT

- Do NOT introduce the `Provider` trait, a `ProviderCmd` enum, or any registry — that
  is Phase 2. This phase only defines wire types and unifies the push tail.
- Do NOT create a new crate. Everything stays in `apps/cli` as a new module. (The
  crate split is explicitly deferred.)
- Do NOT change any provider's CLI grammar, verbs, output shape, trimming, field
  selection, or the `--skip-push` semantics. `cli.rs` should not change.
- Do NOT change the store wire format or `store_client.rs`. `Emission` lowers to the
  exact same JSON `put_work`/`put_edges` already receive.
- Do NOT touch the arXiv provider (it does not exist yet) or the `refs_backfill`
  (Crossref/OpenCitations) paths — those push edges directly and are out of scope.
- Do NOT add `chrono` or any datetime dependency — reuse the existing hand-rolled
  `rfc3339_now`/`days_to_ymd`, just move them to one place.

## Plan

### 1. New module `apps/cli/src/provider/mod.rs` with the serializable wire types

Create `apps/cli/src/provider/mod.rs` and register it in `apps/cli/src/lib.rs`
(`pub mod provider;`). Define the neutral, serializable wire types that every
provider lowers to:

```rust
use serde::{Deserialize, Serialize};

/// A store-ready work record: already lowered to the catalog's neutral vocabulary.
/// Mirrors exactly what `StoreClient::put_work` accepts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkRecord {
    pub source: String,
    pub kind: String,                 // "Work" | "Author" | "Venue" | "Topic" | …
    pub aliases: Vec<Alias>,
    pub attrs: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alias {
    pub namespace: String,
    pub value: String,
}

/// A store-ready citation edge. Mirrors what `StoreClient::put_edges` accepts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeInput {
    pub src: Alias,
    pub dst: Alias,
    pub relation: String,
    pub source: String,
    pub attrs: serde_json::Value,     // serde_json::Value::Null when absent
    pub fetched_at: String,
}

/// Everything a provider verb produces for the store, fully lowered.
/// This is the output half of the (future) subprocess plugin protocol.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Emission {
    pub records: Vec<WorkRecord>,
    pub edges: Vec<EdgeInput>,
}

impl Emission {
    pub fn empty() -> Self { Self::default() }
}
```

Move the duplicated timestamp helpers (`rfc3339_now`, `days_to_ymd`, `is_leap`) out of
`semanticscholar/mapping.rs` into this module (e.g. a `provider::time` submodule or
private fns re-exported), so both providers and `EdgeInput` construction share one
copy. Keep the implementation identical.

Decide the representation faithfully: `put_work`/`put_edges` currently take
`serde_json::Value`. `Emission` types must `serde_json::to_value(...)` into byte-identical
JSON to what is sent today (same field names, same `attrs: null` for absent edge attrs).
Add a unit test asserting the serialized shape matches a hand-written `json!({...})`.

### 2. Lower each provider's mapping to `Emission` instead of `PushBatch`

The goal: the `Entity` enum must NOT appear in the type a verb returns. Each provider
lowers `Entity → kind: &str` *internally* during mapping.

For **OpenAlex** (`openalex/`):
- Add a function (in `openalex/mapping.rs`) that converts the provider's collected
  records + edge pairs into a `provider::Emission` — i.e. fold the existing
  `to_work_record(entity, record)` and `to_edges(pairs)` logic so it yields
  `Vec<WorkRecord>` / `Vec<EdgeInput>`. Records whose `node_kind(entity)` is `None`
  are dropped here (preserving today's "skipped unmappable" behavior — see step 4).
- Change every verb in `openalex/verbs.rs` to return `(Envelope, Emission)` instead of
  `(Envelope, PushBatch)`. Remove the `openalex::PushBatch` type (and its
  re-export in `openalex/mod.rs`).

For **Semantic Scholar** (`semanticscholar/`): the same transformation. Note S2's
edges are already `(citing_alias, cited_alias)` strings in `ns:value` form — lower them
via the existing `split_alias` into `EdgeInput`. Change verbs to return
`(Envelope, Emission)`; remove `semanticscholar::PushBatch`.

Keep `node_kind`, `extract_aliases`, `best_paper_alias`, `reconstruct_abstract`, and all
trimming/shape logic exactly as-is — only the *return container* changes.

### 3. One push path in `main.rs`

Replace `push_batch_to_store`, `push_s2_batch_to_store`, `report_push_summary`,
`report_s2_push_summary` (and the `PushBatch`/`PushSummary` imports for both providers)
with a single pair:

```rust
fn push_emission(store: &StoreClient, em: &Emission) -> PushSummary { … }
fn report_push_summary(s: &PushSummary) { … }
```

`PushSummary` now lives in `provider/mod.rs` (one definition). `push_emission`:
- iterates `em.records`, `serde_json::to_value`s each `WorkRecord`, calls
  `store.put_work`; counts `nodes_pushed`, collects errors.
- if `em.edges` non-empty, `serde_json::to_value`s them and calls `store.put_edges`;
  sets `edges_pushed`.
- returns the same `PushSummary { nodes_pushed, edges_pushed, skipped_unmappable, errors }`.

`skipped_unmappable` is now computed at lowering time (step 2 drops unmappable records),
so the provider must report how many it dropped. Carry that count on `Emission` (add a
`pub skipped_unmappable: usize` field, or return it alongside). Preserve the existing
stderr message format exactly:
`"push: {n} node(s) stored, {e} edge(s) stored, {s} skipped (unmappable kind)"`.

Update both provider dispatch arms (`Namespace::Openalex`, `Namespace::Semanticscholar`)
to call the single `push_emission`/`report_push_summary`, keeping the existing
`if !opts.skip_push { … }` guard and the `if !summary.errors.is_empty() { exit(1) }`
behavior identical.

### 4. Preserve "skipped unmappable" semantics

Today, OpenAlex can fetch entities (Institutions/Publishers/Funders/Keywords) whose
`node_kind` is `None`; those are counted as `skipped_unmappable` and not pushed. Keep
this: when lowering to `Emission`, drop such records and increment the skipped count.
S2 never produces `None` kinds, so its skipped count stays 0. Add/keep a test covering
an unmappable OpenAlex entity → it is excluded from `records` and counted as skipped.

## Files to Modify

- `apps/cli/src/provider/mod.rs` — NEW: `WorkRecord`, `Alias`, `EdgeInput`, `Emission`,
  `PushSummary`, moved timestamp helpers, unit tests for serialized shape.
- `apps/cli/src/lib.rs` — add `pub mod provider;`.
- `apps/cli/src/openalex/mod.rs` — remove `PushBatch`; keep `PushSummary` removed here
  (now in `provider`). Update re-exports.
- `apps/cli/src/openalex/mapping.rs` — add `to_emission`-style lowering; reuse shared
  timestamp helper.
- `apps/cli/src/openalex/verbs.rs` — verbs return `(Envelope, Emission)`.
- `apps/cli/src/semanticscholar/mod.rs` — remove `PushBatch`/`PushSummary`.
- `apps/cli/src/semanticscholar/mapping.rs` — lower to `Emission`; drop the local
  timestamp helpers (now shared).
- `apps/cli/src/semanticscholar/verbs.rs` — verbs return `(Envelope, Emission)`.
- `apps/cli/src/main.rs` — single `push_emission` + `report_push_summary`; update both
  dispatch arms; fix imports.

## Verification

```bash
cargo build --bin braincrawl --bin braincrawl-server
cargo test -p braincrawl-cli
cargo clippy -p braincrawl-cli --no-deps 2>&1 | grep -i "warning\|error" || echo "clippy clean"
```

All existing provider tests must still pass unchanged. The push-summary stderr strings
and exit codes must be identical to before.

## Out of Scope

- The `Provider` trait, `ProviderCmd` union enum, and registry dispatch (Phase 2).
- The arXiv provider (Phase 3).
- Splitting providers into their own crate (deferred indefinitely; modules only).
- Any change to `refs_backfill` (Crossref/OpenCitations) push behavior.
- Any subprocess/plugin loader code.

## Notes

- This is the single biggest churn-reduction in the chain: 4 functions + 2 types
  collapse to 1 function + 1 type. Reviewer should diff `main.rs` carefully — the
  behavior must be identical, only the plumbing unified.
- The `Emission` serde round-trip test is load-bearing for Phase 2/future plugins:
  it pins the JSON contract a subprocess provider will speak.

## Surface after this phase

- `apps/cli/src/provider/mod.rs` exists and is `pub mod provider;` in `lib.rs`, exporting:
  - `provider::WorkRecord { source: String, kind: String, aliases: Vec<Alias>, attrs: Value }`
  - `provider::Alias { namespace: String, value: String }`
  - `provider::EdgeInput { src, dst: Alias, relation, source: String, attrs: Value, fetched_at: String }`
  - `provider::Emission { records: Vec<WorkRecord>, edges: Vec<EdgeInput>, skipped_unmappable: usize }`
    with `Emission::empty()`, all deriving `Serialize`/`Deserialize`.
  - `provider::PushSummary { nodes_pushed: usize, edges_pushed: u64, skipped_unmappable: usize, errors: Vec<String> }`.
  - Shared timestamp helper(s) (`rfc3339_now`) usable by provider mappings.
- Every OpenAlex and Semantic Scholar verb returns `(Envelope, Emission)`. The
  provider-private `PushBatch` types are GONE.
- `main.rs` exposes exactly one `push_emission(&StoreClient, &Emission) -> PushSummary`
  and one `report_push_summary(&PushSummary)`; no per-provider push/report fns remain.
- Each provider lowers `Entity → kind` internally; `Entity` never appears in a verb's
  return type or in the push path.
- Negative space: the `Provider` trait and `ProviderCmd` do NOT exist yet. CLI grammar
  in `cli.rs` is unchanged. `refs_backfill` still pushes edges via its own path.
  No new crate exists.
