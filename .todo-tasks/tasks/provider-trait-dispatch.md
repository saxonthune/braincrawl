# Provider refactor — Phase 2: the Provider trait + ProviderCmd input contract

## Motivation

Phase 1 gave us the OUTPUT half of a provider's contract: a serializable `Emission`.
This phase adds the INPUT half — a serializable `ProviderCmd` union enum — and ties
both ends together behind a `Provider` trait, so `main.rs` dispatches through a uniform
seam instead of a hand-written match arm per provider.

Why this shape: the explicit long-term goal is subprocess plugins (a provider is any
executable that reads a command and writes an `Emission`). For that, BOTH the command
and the result must be serializable data. After this phase, a subprocess provider is
"serialize `ProviderCmd` to a child process, deserialize `Emission` from its stdout" —
a `Provider` impl with no new contract to design. We are NOT building that loader here;
we are leaving the seam it plugs into.

This remains a behavior-preserving refactor: the CLI grammar, help text, output, and
push behavior are unchanged.

## Do NOT

- Do NOT build any subprocess/exec/plugin loader, plugin discovery, or `$PATH`
  scanning. This phase only defines the trait + command enum + in-process registry.
- Do NOT create a new crate. The trait lives in `apps/cli/src/provider/`. (Crate split
  deferred.)
- Do NOT change the user-facing CLI grammar in a breaking way. The clap subcommand
  enums (`OpenalexCmd`, `SemanticscholarCmd`) stay as the typed surface with their
  current names, args, and help text; they now *map into* `ProviderCmd`.
- Do NOT change output shape, trimming, `--skip-push`, or exit-code behavior.
- Do NOT touch the store wire format or `Emission` shape from Phase 1.
- Do NOT modify `refs_backfill` (Crossref/OpenCitations); they are not providers in
  this sense (DOI-keyed edge backfill, no `get`/`search`). Leave them on their own arm.
- Do NOT add the arXiv provider — that is Phase 3, which implements this trait.

## Plan

### 1. Define `ProviderCmd` — the serializable union of all provider verbs

In `apps/cli/src/provider/mod.rs` (or a `provider/command.rs` submodule), add:

```rust
use serde::{Deserialize, Serialize};

/// The union of every provider verb. Individual providers support a subset and
/// return `ProviderError::UnsupportedVerb` for the rest. This is the input half of
/// the (future) subprocess plugin protocol, hence Serialize/Deserialize.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "verb", rename_all = "snake_case")]
pub enum ProviderCmd {
    Get { id: String },
    Search { entity: Option<String>, query: String },
    Find { entity: String, filters: Vec<String> },
    Autocomplete { entity: String, q: String },
    CitedBy { id: String },
    Refs { id: String },
}
```

Note `Search.entity` is `Option<String>`: OpenAlex/S2 pass `Some(entity)`; a
works-only provider (arXiv, Phase 3) passes `None`. Keep the enum the full union even
though no single provider uses all six — that is the point of a shared contract.

### 2. Define the `Provider` trait + `ProviderError`

```rust
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("provider '{provider}' does not support verb '{verb}'")]
    UnsupportedVerb { provider: &'static str, verb: &'static str },
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error>),
}

pub trait Provider {
    fn name(&self) -> &'static str;
    fn dispatch(&self, cmd: ProviderCmd, opts: &OutputOpts)
        -> Result<(Envelope, Emission), ProviderError>;
}
```

`dispatch` is the single entry point. Each provider matches on `ProviderCmd`, calls its
existing verb functions (now returning `(Envelope, Emission)` from Phase 1), and maps
unsupported variants to `ProviderError::UnsupportedVerb`. Provider verb errors
(`OpenAlexError`, `SemanticScholarError`) convert into `ProviderError::Other` (add
`From` impls or box them).

### 3. Implement `Provider` for OpenAlex and Semantic Scholar

Add a thin struct per provider that owns its client and implements `Provider`:

- `openalex/mod.rs`: `pub struct OpenAlexProvider { client: OpenAlexClient }` (or hold
  the client by value/config). `name()` → `"openalex"`. `dispatch` matches all six
  variants onto `verbs::get/search/find/autocomplete/cited_by/refs`. For `Search`,
  unwrap `entity` (OpenAlex requires it — if `None`, return an error or default to
  `"works"`; preserve current behavior where the entity arg is required by clap, so
  `None` should not occur for OpenAlex).
- `semanticscholar/mod.rs`: `pub struct SemanticScholarProvider { client }`.
  `name()` → `"semanticscholar"`. Supports `Get`, `Search`, `CitedBy`, `Refs`;
  returns `UnsupportedVerb` for `Find`/`Autocomplete`.

Keep the verb functions as-is (free functions); the provider struct just routes to them.

### 4. Map clap enums → `ProviderCmd` and dispatch through a registry in `main.rs`

Keep `OpenalexCmd`/`SemanticscholarCmd` in `cli.rs` unchanged. In `main.rs`, add a
small conversion from each clap enum into `ProviderCmd` (a `From`/`into_cmd` helper),
then collapse the two provider dispatch arms into one uniform flow:

```rust
fn run_provider(p: &dyn Provider, cmd: ProviderCmd, store: &StoreClient, opts: &OutputOpts)
    -> Result<(), Box<dyn std::error::Error>>
{
    let (envelope, emission) = p.dispatch(cmd, opts)?;
    render(&envelope, opts);
    if !opts.skip_push {
        let summary = push_emission(store, &emission);   // from Phase 1
        report_push_summary(&summary);
        if !summary.errors.is_empty() { std::process::exit(1); }
    }
    Ok(())
}
```

The `Namespace::Openalex(oa)` arm constructs `OpenAlexProvider`, converts `oa.cmd` →
`ProviderCmd`, and calls `run_provider`. Same for `Namespace::Semanticscholar`. A
registry (e.g. a `match` on namespace that yields a `Box<dyn Provider>` + `ProviderCmd`)
is fine — a full name-keyed `Vec<Box<dyn Provider>>` is optional and may be deferred,
but the dispatch MUST go through `Provider::dispatch`, not bespoke per-provider code.

Preserve exactly: the `OpenAlexClient::new(config.openalex_api_key)` /
`SemanticScholarClient::new(config.semanticscholar_api_key)` construction, the store
construction, render, push, and exit-code behavior.

### 5. Tests

- Unit test `ProviderCmd` serde round-trips (the tagged JSON shape is the input
  protocol — pin it, e.g. `{"verb":"get","id":"…"}`).
- Test that `SemanticScholarProvider::dispatch(ProviderCmd::Find{…})` returns
  `UnsupportedVerb`.
- Keep all existing provider verb/mapping/shape tests passing untouched.

## Files to Modify

- `apps/cli/src/provider/mod.rs` (+ optional `provider/command.rs`) — `ProviderCmd`,
  `Provider`, `ProviderError`, round-trip + unsupported-verb tests.
- `apps/cli/src/openalex/mod.rs` — `OpenAlexProvider` struct + `impl Provider`.
- `apps/cli/src/semanticscholar/mod.rs` — `SemanticScholarProvider` struct + `impl Provider`.
- `apps/cli/src/main.rs` — clap-enum → `ProviderCmd` conversion, `run_provider`,
  collapsed dispatch arms.
- `apps/cli/src/lib.rs` — export the new provider structs if needed.

## Verification

```bash
cargo build --bin braincrawl --bin braincrawl-server
cargo test -p braincrawl-cli
cargo clippy -p braincrawl-cli --no-deps 2>&1 | grep -i "warning\|error" || echo "clippy clean"
```

Manual smoke (optional, needs a running server): `braincrawl openalex get W2031938753`
and `braincrawl semanticscholar search papers "salinization"` behave exactly as before.

## Out of Scope

- Subprocess plugin loader / `$PATH` discovery / exec protocol (future task; this phase
  only leaves the seam).
- A new crate for providers (deferred; modules only).
- arXiv (Phase 3).
- Reworking `refs_backfill` into the `Provider` trait.

## Notes

- The union enum means `ProviderCmd` has variants no single provider uses — intended.
  It is the shared vocabulary; providers advertise support via `dispatch` returning
  `UnsupportedVerb`.
- Reviewer focus: confirm clap help text / arg names are unchanged (the typed enums
  remain the user surface), and that dispatch now flows through `Provider::dispatch`.

## Surface after this phase

> Triage Phase 3 (arXiv) against this Surface, not live code.

- `provider::ProviderCmd` exists — a `#[serde(tag = "verb", rename_all = "snake_case")]`
  enum with variants `Get { id }`, `Search { entity: Option<String>, query }`,
  `Find { entity, filters }`, `Autocomplete { entity, q }`, `CitedBy { id }`,
  `Refs { id }`; Serialize/Deserialize.
- `provider::Provider` trait exists:
  `fn name(&self) -> &'static str` and
  `fn dispatch(&self, cmd: ProviderCmd, opts: &OutputOpts) -> Result<(Envelope, Emission), ProviderError>`.
- `provider::ProviderError` exists with at least `UnsupportedVerb { provider, verb }`
  and `Other`.
- `OpenAlexProvider` and `SemanticScholarProvider` structs exist and `impl Provider`;
  S2 returns `UnsupportedVerb` for `Find`/`Autocomplete`.
- `main.rs` dispatches every provider namespace through `Provider::dispatch` via a
  shared `run_provider` helper that renders, then pushes the `Emission` via Phase 1's
  `push_emission` under the `--skip-push` guard.
- A new provider is added by: (1) a module implementing `Provider`, (2) a clap
  subcommand enum in `cli.rs` that maps into `ProviderCmd`, (3) one dispatch arm in
  `main.rs` constructing the provider and calling `run_provider`.
- Phase-1 surface (`Emission`, `WorkRecord`, `EdgeInput`, `Alias`, `PushSummary`,
  `push_emission`, shared `rfc3339_now`) remains and is unchanged.
- Negative space: no subprocess loader exists; no providers crate exists; `refs_backfill`
  is still a separate non-`Provider` path; CLI grammar for existing providers is
  unchanged.
