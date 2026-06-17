# braincrawl openalex — read vocabulary (context-only)

## Motivation

Add the `braincrawl openalex` provider namespace (doc02.06.01.01): the deliberate
fork that queries the OpenAlex API directly. This phase delivers the full base
vocabulary as **read-only-to-context** for all entities — `get`, `search`, `find`,
`autocomplete`, `cited-by`, `refs` — returning the structured `Envelope` from the
foundation phase. Persistence (push) is the next phase; this phase leaves a clean
hook for it but writes nothing to the store.

The upstream contract (endpoints, filter keys, ID forms, pagination, rate limits)
is the `openalex-reference` skill at `.claude/skills/openalex-reference/reference.md`
— read it; do not guess filter keys.

## Do NOT

- Do NOT push anything to the metadata server in this phase. No `StoreClient` calls.
  `--skip-push` is accepted (from `GlobalArgs`) but push is simply not wired yet.
- Do NOT hit the live OpenAlex API in tests or in the verification gate. Unit-test the
  pure functions against fixture JSON; live calls are manual only.
- Do NOT guess filter keys. Validate against a static per-entity allowlist derived
  from the reference; an unknown key is a loud error, never a silent empty result.
- Do NOT reconstruct abstracts by default — only when `--abstract` is passed
  (bulky). Default-trim records to a curated top-level field set.
- Do NOT change the foundation surface (envelope, config, store client). Consume them.

## Plan

### 1. Register the namespace

- Add an `Openalex` variant to the `Namespace` enum (`apps/cli/src/cli.rs`) with the
  six verb subcommands: `get <id>`, `search <entity> <query>`, `find <entity> <k:v…>`,
  `autocomplete <entity> <q>`, `cited-by <id>`, `refs <id>`.
- New module tree under `apps/cli/src/openalex/`.

### 2. Entity model (`src/openalex/entity.rs`)

- `enum Entity { Works, Authors, Sources, Institutions, Topics, Keywords,
  Publishers, Funders, Concepts }` with path segment (`/works`, …) and parse-from-arg.
- `infer_entity(id: &str) -> (Entity, String)` for `get`: map ID prefix / external-ID
  form to entity + the OpenAlex single-entity path id (e.g. `W…`→works,
  `doi:`/`https://doi.org/`→works, `A…`/ORCID→authors, `S…`/ISSN→sources, `I…`/ROR→
  institutions, `T…`→topics, `P…`→publishers, `F…`→funders, `C…`→concepts). Use the
  ID-forms table in the reference.

### 3. Filter allowlist (`src/openalex/filters.rs`)

- A static map `Entity -> &[&str]` of valid filter keys (works set is the large one in
  the reference; other entities are smaller). `validate_filters(entity, &[KV]) ->
  Result<String>` returns the encoded `filter=` value or errors listing the bad key(s).

### 4. OpenAlex client (`src/openalex/client.rs`)

- `OpenAlexClient { base_url, api_key: Option<String>, http: reqwest::blocking::Client }`
  (base_url defaults to `https://api.openalex.org`, overridable for tests).
- Methods: `get_one(entity, id, select) -> Value`; `list(entity, params) -> ListPage`
  (params: filter/search/sort/select/per_page/cursor); `autocomplete(entity, q) -> Value`.
- Deterministic plumbing: attach `api_key` when present; retry `429`/5xx with bounded
  backoff; `list` supports cursor paging — `--all` drains until `next_cursor` is null,
  otherwise honor `--limit`. Parse upstream `meta` into the envelope (`count`,
  `next_cursor`).

### 5. Trimming + envelope (`src/openalex/shape.rs`)

- Per-entity `trim(entity, &Value) -> Value` keeping a curated top-level field set
  (e.g. works: id, doi, title, publication_year, authorships→names, primary_location
  source, cited_by_count, open_access). `--full` bypasses trimming. `--fields` (from
  foundation) projects after trim.
- `reconstruct_abstract(&Value) -> Option<String>` from `abstract_inverted_index`,
  applied only when `--abstract` is set.
- Build the foundation `Envelope` with `QueryMeta { entity, resolved_filter, url }`.

### 6. Verbs (`src/openalex/verbs.rs`)

- Implement the six verbs producing an `Envelope`:
  - `get`: `infer_entity` → `get_one` → trim → single-result envelope.
  - `search`/`find`/`autocomplete`: `list`/`autocomplete` → trim → envelope.
  - `cited-by <id>`: `list(works, filter=cites:<id>)`.
  - `refs <id>`: `get_one(works, id, select=referenced_works)` then batch-fetch the
    referenced ids via `list(works, filter=ids.openalex:<a|b|…>)` in chunks of ≤50.
- **Push hook (no-op this phase):** each verb returns, alongside its `Envelope`, the
  set of fetched raw records (and for `cited-by`/`refs` the citation pairs) as a
  `PushBatch` value. This phase renders the envelope and discards the `PushBatch`.
  Define `PushBatch` now so the next phase only has to consume it.

## Files to Modify

- `apps/cli/src/cli.rs` — add `Openalex` namespace + verb args.
- `apps/cli/src/main.rs` — dispatch openalex verbs.
- `apps/cli/src/openalex/mod.rs` — module wiring + `PushBatch` type.
- `apps/cli/src/openalex/entity.rs` — `Entity`, `infer_entity`.
- `apps/cli/src/openalex/filters.rs` — allowlist + `validate_filters`.
- `apps/cli/src/openalex/client.rs` — `OpenAlexClient`.
- `apps/cli/src/openalex/shape.rs` — trim, abstract reconstruction, envelope build.
- `apps/cli/src/openalex/verbs.rs` — the six verbs.
- `apps/cli/Cargo.toml` — no new runtime deps expected (reqwest blocking already present).
- `apps/cli/tests/openalex_shape.rs` — fixture-based unit tests.

## Verification

```bash
cargo build -p braincrawl-cli
cargo test -p braincrawl-cli
cargo run -p braincrawl-cli -- openalex find works publication_year:2020 is_oa:true --limit 1 --help
```

Tests use fixture JSON under `apps/cli/tests/fixtures/` (a sample work, a list page,
an autocomplete page — captured/abbreviated, committed) and assert: `infer_entity`
mapping for each ID form; `validate_filters` accepts known keys and errors on an
unknown key; `trim` keeps the curated fields and `--full` does not; abstract
reconstruction orders words correctly; the envelope carries `count`/`resolved_filter`.
No network in tests.

## Out of Scope

- Pushing to the metadata server (next phase, `braincrawl-openalex-push`).
- Entities beyond query for non-pushable kinds — they all return to context here; the
  push boundary is enforced in the next phase.
- The `--text` polished renderer beyond the foundation stub.

## Notes

- Read `.claude/skills/openalex-reference/reference.md` for exact filter keys, ID
  forms, pagination, and the `meta` shape. OpenAlex returns `200` + empty results on a
  bad filter — that is exactly why `validate_filters` exists.
- Keep `OpenAlexClient.base_url` injectable so a future wiremock/fixture server can be
  pointed at it without code changes.

## Surface after this phase

- `braincrawl openalex` namespace with verbs `get`, `search`, `find`, `autocomplete`,
  `cited-by`, `refs`, each returning the foundation `Envelope` to context.
- `apps/cli/src/openalex/entity.rs`: `Entity` enum + `infer_entity(&str)->(Entity,String)`.
- `apps/cli/src/openalex/filters.rs`: `validate_filters(Entity,&[KV])->Result<String>`
  backed by a static per-entity allowlist.
- `apps/cli/src/openalex/client.rs`: `OpenAlexClient` (injectable `base_url`, `api_key`,
  429 backoff, cursor paging honoring `--all`/`--limit`).
- `apps/cli/src/openalex/shape.rs`: `trim(Entity,&Value)->Value`, `--full` bypass,
  `reconstruct_abstract`.
- `apps/cli/src/openalex/mod.rs`: a public `PushBatch { records: Vec<(Entity,Value)>,
  edges: Vec<(String,String)> }` (citing→cited alias pairs) returned by each verb and
  currently discarded after rendering. This is the hook the push phase consumes.
- Negative space: NOTHING is written to the store; `StoreClient` is not called from
  openalex; `--skip-push` is parsed but has no effect yet.
