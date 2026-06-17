# Add Semantic Scholar provider core (CLI namespace + push)

## Motivation

Dogfooding the Mesopotamia case study exposed a structural gap in OpenAlex: its
MAG-derived records for older humanities works frequently have **no abstract** and
**zero `referenced_works`** — backward `refs` returned empty for landmark works
(W99391817, W94828380). Semantic Scholar (S2) is a full citation-graph provider with
stronger abstract coverage and better humanities reach. This phase adds S2 as a second
provider fork (`braincrawl semanticscholar …`) that improves **L1** (abstracts) and
**L2** (citation edges) at once, pushing into the *same* neutral store so coverage
accumulates in one graph.

This is **Phase 1 of a 2-phase chain** and is **code only**. Phase 2
(`semantic-scholar-skill-setup`) writes the consumer-facing config section, SETUP.md,
and the rhidoc provider doc. **Do not touch any docs or skills in this phase.**

It mirrors the existing OpenAlex provider almost exactly. Read
`apps/cli/src/openalex/{mod,client,entity,shape,mapping,verbs}.rs` first — the new
module is a near-parallel of it. Match its structure, naming, and conventions.

## Do NOT

- **Do NOT edit any `.md`, `.rhidoc/`, or `.claude/skills/` file.** No docs, no SKILL.md,
  no SETUP.md, no MANIFEST. Those are Phase 2. This phase is Rust + test fixtures only.
- **Do NOT fork the canonical graph.** The entire point is that an S2 paper merges onto
  its OpenAlex twin. Always push the *same* `doi:<bare-value>` alias namespace and bare
  value OpenAlex uses (`extract_aliases` in `openalex/mapping.rs` strips
  `https://doi.org/` → bare DOI). A botched alias mapping that registers `doi:https://doi.org/10.x`
  vs `doi:10.x` would fork the corpus — exactly what the shared store exists to prevent.
- **Do NOT auto-explode embedded authors into Author nodes.** Author nodes are created
  ONLY when the caller explicitly queries the `authors` entity (`get <authorId>` /
  `search authors …`), exactly as OpenAlex does — querying a paper pushes a `Work` node
  whose `attrs` retains the full `authors` array, but does not push per-author nodes.
  (Auto-promotion of embedded authorships would create a stub-node explosion and diverge
  from OpenAlex behavior.)
- **Do NOT refactor the OpenAlex module or the shared `output.rs`/`store_client.rs`.**
  Add a parallel S2 module; reuse `Envelope`/`render`/`StoreClient` as-is.
- **Do NOT invent filter keys.** S2 has no `find`/`autocomplete` analog in this phase —
  only `get`, `search`, `cited-by`, `refs`.

## Plan

### 1. Config key (`apps/cli/src/config.rs`)

Add `semanticscholar_api_key: Option<String>` to both `Config` and `ConfigFile`, mirroring
`openalex_api_key`. In `resolve()`, read env `BRAINCRAWL_SEMANTICSCHOLAR_API_KEY` first,
then fall back to `file.semanticscholar_api_key`. Precedence stays env > file > none.

### 2. Register the module (`apps/cli/src/lib.rs`)

Add `pub mod semanticscholar;`.

### 3. New module `apps/cli/src/semanticscholar/`

Create these files, mirroring the OpenAlex shapes:

- **`mod.rs`** — `pub mod {client, entity, shape, mapping, verbs};`, a
  `SemanticScholarError` enum (`Http`, `Api { status, body }`, `InferFailed`,
  `UnknownEntity`) + `Result<T>`, and `PushBatch`/`PushSummary` structs identical in
  shape to OpenAlex's (records as `Vec<(Entity, Value)>`, edges as `Vec<(String, String)>`
  where each string is a store alias in `ns:value` form). `PushBatch::empty()`.

- **`entity.rs`** — `enum Entity { Papers, Authors }`. `path_segment()` → `"paper"` /
  `"author"` (S2 uses singular path segments). `parse(s)` accepts
  `papers|paper|works|work` → `Papers`, `authors|author` → `Authors`.
  `infer_entity(id) -> Result<(Entity, String)>` returning `(entity, s2_path_id)` where
  `s2_path_id` is the form S2's path accepts:
    - Store lowercase forms → translate to S2 native path forms (Papers):
      `doi:X`→`DOI:X`, `corpusid:X`→`CorpusId:X`, `arxiv:X`→`ARXIV:X`, `mag:X`→`MAG:X`,
      `pmid:X`→`PMID:X`, `pmcid:X`→`PMCID:X`, `s2:X`→`X` (bare paperId).
    - S2 native forms passed through unchanged (Papers): `DOI:`, `ARXIV:`, `CorpusId:`,
      `MAG:`, `PMID:`, `PMCID:`, `ACL:`, `DBLP:`, `URL:`.
    - A bare 40-char lowercase-hex string → Papers, bare (S2 `paperId`).
    - `s2author:X` → Authors, path `X`.
    - Anything else (incl. ambiguous bare numerics — could be CorpusId or authorId) →
      `InferFailed`, **fail loudly** asking for an explicit prefix. Match the OpenAlex
      fail-loud ethos.
  Include unit tests covering each id form (mirror `openalex/entity.rs` tests).

- **`client.rs`** — `SemanticScholarClient { base_url, api_key, http: blocking::Client }`,
  `new(api_key)`, `with_base_url(url)` (for fixture tests). Base
  `https://api.semanticscholar.org/graph/v1`. When `api_key` is `Some`, send it as the
  `x-api-key` **header** (NOT a query param). Methods:
    - `get_paper(path_id, fields) -> (Value, url)` → `GET /paper/{path_id}?fields=…`
    - `get_author(path_id, fields) -> (Value, url)` → `GET /author/{path_id}?fields=…`
    - `search_papers(query, fields, offset, limit) -> Value` → `GET /paper/search?query=…`
      (response: `{total, offset, next, data:[…]}`)
    - `search_authors(query, fields, offset, limit) -> Value` → `GET /author/search?query=…`
    - `citations(path_id, fields, offset, limit) -> Value` → `GET /paper/{id}/citations`
      (response: `{offset, next, data:[{citingPaper:{…}}]}`)
    - `references(path_id, fields, offset, limit) -> Value` → `GET /paper/{id}/references`
      (response: `{offset, next, data:[{citedPaper:{…}}]}`)
  Reuse the OpenAlex retry pattern: retry `429`/5xx up to 3 times with 1s/2s/4s backoff;
  non-success non-retryable → `Api { status, body }`.

- **`shape.rs`** — curated-field trimming + envelope build, mirroring OpenAlex's
  `build_envelope`/`trim` but simpler:
    - `curated_fields(Papers)`: `paperId, externalIds, title, abstract, year,
      publicationDate, venue, citationCount, referenceCount, authors, openAccessPdf`.
    - `curated_fields(Authors)`: `authorId, externalIds, name, affiliations, paperCount,
      citationCount, hIndex`.
    - `trim(entity, v, opts)` — same allowlist approach; `--full` bypasses.
    - Abstract handling: S2 returns plain `abstract` text. It is **already plain** — no
      inverted-index reconstruction. When `opts.include_abstract` is false, drop the
      `abstract` field from trimmed output (it is bulky); when true, keep it. (This is the
      inverse of OpenAlex, which injects on request — here we *omit* unless requested, so
      `--abstract` stays the bulky-on-demand flag.)
    - `build_envelope(entity, results_raw, count, next_cursor, resolved_filter, url, opts)`
      returning the shared `crate::output::Envelope`. Honor `--limit`/`--all` truncation
      exactly as OpenAlex does.

- **`mapping.rs`** — the canonical-id crux:
    - `node_kind(Papers)=Some("Work")`, `node_kind(Authors)=Some("Author")`.
    - `extract_aliases(Papers, record)`: push `s2:<paperId>`, then from
      `externalIds`: `DOI`→`doi:<bare>` (strip any `https://doi.org/`), `ArXiv`→`arxiv:`,
      `MAG`→`mag:`, `PubMed`→`pmid:`, `PubMedCentral`→`pmcid:`, `CorpusId`→`corpusid:`
      (CorpusId may be numeric — stringify). Skip keys that are absent. Bare values only.
    - `extract_aliases(Authors, record)`: `s2author:<authorId>`, and
      `externalIds.ORCID`→`orcid:<bare>` if present.
    - `to_work_record(entity, record)` → `{ "source": "semanticscholar", "kind",
      "aliases", "attrs": record }`, `None` when `node_kind` is `None` (never, here).
    - `to_edges(pairs)` — pairs are `(citing_alias, cited_alias)` where each alias is a
      full `ns:value` store string already chosen by the verb (see below). Emit
      `{src:{namespace,value}, dst:{namespace,value}, relation:"cites",
      source:"semanticscholar", attrs:null, fetched_at:<rfc3339>}`. Split each `ns:value`
      on the first `:`. Reuse a local `rfc3339_now()` (copy OpenAlex's, or factor a tiny
      shared helper — copying is acceptable to avoid cross-module refactor).
    - Unit tests: alias extraction for a paper with DOI+ArXiv+CorpusId; author with ORCID;
      edge shape; DOI normalization to bare value (the merge guarantee).

- **`verbs.rs`** — `get`, `search`, `cited_by`, `refs`, each returning
  `(Envelope, PushBatch)`:
    - `get(client, id, opts)`: infer entity; fetch paper or author with curated fields
      (request `externalIds,abstract` etc. so aliases are extractable); 1-record envelope;
      push that single node.
    - `search(client, entity_str, query, opts)`: parse entity; page `search_papers`/
      `search_authors` honoring `--limit`/`--all` via `offset`/`next`; push all results
      as nodes of that kind.
    - `cited_by(client, id, opts)`: infer the seed (must be a paper); page `citations`;
      each `data[].citingPaper` is a full paper record → push as a `Work` node (so it
      merges) AND emit edge `(citing_alias, seed_alias)`. **Choose each edge endpoint's
      alias = `doi:<bare>` if that paper has a DOI in `externalIds`, else `s2:<paperId>`**
      — this maximizes merge with OpenAlex edges. The seed alias is computed the same way
      from the seed's own record (fetch it once for its externalIds, or derive from the
      requested id).
    - `refs(client, id, opts)`: same, but `data[].citedPaper`, edge `(seed_alias, cited_alias)`.
    - Empty results → empty envelope + `PushBatch::empty()`, no error.

### 4. CLI namespace (`apps/cli/src/cli.rs`)

Add to the `Namespace` enum:
```rust
#[command(about = "Query the Semantic Scholar graph API")]
Semanticscholar(SemanticscholarArgs),
```
Define `SemanticscholarArgs { #[command(subcommand)] cmd: SemanticscholarCmd }` and
`SemanticscholarCmd` with: `Get { id }`, `Search { entity, query }`,
`CitedBy { id }` (`#[command(name = "cited-by")]`), `Refs { id }`. Mirror the OpenAlex
arg doc-comments. No new global flags — the existing `GlobalArgs`/`OutputOpts` apply.

### 5. Dispatch + push (`apps/cli/src/main.rs`)

Add a `Namespace::Semanticscholar(s2)` arm mirroring the `Openalex` arm: build
`SemanticScholarClient::new(config.semanticscholar_api_key)` and a `StoreClient`, match the
subcommand to the verb, `render(&envelope, &opts)`, and on `!opts.skip_push` push the batch.
Add a `push_s2_batch_to_store(store, batch)` parallel to `push_batch_to_store` (it calls the
S2 `mapping::to_work_record`/`to_edges`); reuse `report_push_summary`. Import S2 types under
their own `use` lines. Keep the OpenAlex path untouched.

### 6. Tests + fixtures

Add fixtures under `apps/cli/tests/fixtures/`: `s2_paper.json` (a paper with
`externalIds` incl. DOI+ArXiv+CorpusId and a plain `abstract`), `s2_paper_search.json`
(`{total,offset,next,data:[…]}`), `s2_citations.json` (`{data:[{citingPaper:{…}}]}`),
`s2_author.json`. Add an integration test file `apps/cli/tests/semanticscholar_push.rs`
(mirror `openalex_push.rs`) and `semanticscholar_shape.rs` (mirror `openalex_shape.rs`)
asserting: DOI normalizes to a bare `doi:` alias (merge guarantee), `s2:`/`corpusid:`
aliases are registered, a paper maps to `Work` with `source:"semanticscholar"`, an author
maps to `Author` with `s2author:`/`orcid:`, citation edges have `relation:"cites"` and
`source:"semanticscholar"`, and abstract is omitted unless `--abstract`.

## Files to Modify

- `apps/cli/src/config.rs` — add `semanticscholar_api_key` (struct + ConfigFile + resolve)
- `apps/cli/src/lib.rs` — `pub mod semanticscholar;`
- `apps/cli/src/cli.rs` — `Semanticscholar` namespace + subcommands
- `apps/cli/src/main.rs` — dispatch arm + `push_s2_batch_to_store`
- `apps/cli/src/semanticscholar/mod.rs` — module root, error, PushBatch/PushSummary
- `apps/cli/src/semanticscholar/entity.rs` — Entity + infer_entity (+ tests)
- `apps/cli/src/semanticscholar/client.rs` — HTTP client (x-api-key header, retry, paging)
- `apps/cli/src/semanticscholar/shape.rs` — trim + build_envelope
- `apps/cli/src/semanticscholar/mapping.rs` — aliases/node_kind/to_work_record/to_edges (+ tests)
- `apps/cli/src/semanticscholar/verbs.rs` — get/search/cited-by/refs
- `apps/cli/tests/fixtures/s2_*.json` — new fixtures
- `apps/cli/tests/semanticscholar_push.rs`, `apps/cli/tests/semanticscholar_shape.rs` — tests

## Verification

```bash
cargo build --bin braincrawl
cargo test -p braincrawl-cli
```

Both must pass. The new tests must assert the DOI-merge guarantee (bare `doi:` value) and
the `source:"semanticscholar"` provenance on both nodes and edges.

## Out of Scope

- Skill / SETUP.md / rhidoc docs — that is Phase 2 (`semantic-scholar-skill-setup`).
- Crossref / OpenCitations backward-edge backfill (separate task).
- PDF/fulltext retrieval; provider auto-fallback / cross-provider merge policy.
- Auto-promoting embedded authorships to Author nodes; field-level conflict resolution
  beyond "same canonical node, union edges, keep both providers' attrs under provenance."
- A `find`/`autocomplete` verb for S2 (S2 has no clean analog; not needed by the funnel).

## Notes

- The merge is the whole game: verify a paper that shares a DOI with an existing OpenAlex
  node lands on the *same* canonical node (the server's union-find does the merge; our job
  is to hand it the identical `doi:<bare>` alias). The push tests are the guard.
- `cited-by`/`refs` push the related papers as full nodes precisely so the canonical merge
  happens before the edge attaches — bare edges alone wouldn't merge S2 paperIds onto
  OpenAlex nodes.
- S2 rate-limits hard without a key (shared pool ~1 req/s). The retry/backoff matters; a
  key raises the limit. Configuring the key is Phase 2's setup-help job.

## Surface after this phase

- **New CLI namespace** `braincrawl semanticscholar` with subcommands: `get <id>`,
  `search <entity> <query>` (entity ∈ {papers, authors}), `cited-by <id>`, `refs <id>`.
  All honor the existing global flags (`--text/--json/--limit/--all/--fields/--full/--abstract/--skip-push`).
- **New config field** `Config.semanticscholar_api_key: Option<String>`, resolved
  env `BRAINCRAWL_SEMANTICSCHOLAR_API_KEY` > `~/.config/braincrawl/config.toml`
  key `semanticscholar_api_key` > none. Sent to S2 as the `x-api-key` header.
- **Push provenance** `source = "semanticscholar"` on all S2-pushed nodes and edges
  (visible in `braincrawl stats` under "assertions by source").
- **Node kinds**: S2 papers → `Work`, S2 authors → `Author`.
- **Alias namespaces** S2 registers: `s2` (paperId), `corpusid`, `doi`, `arxiv`, `mag`,
  `pmid`, `pmcid` (papers); `s2author`, `orcid` (authors). DOI overlap merges S2↔OpenAlex
  onto one canonical node.
- **Module** `apps/cli/src/semanticscholar/{mod,client,entity,shape,mapping,verbs}.rs`,
  registered in `lib.rs`.
- **Negative space (Phase 2 relies on this):** No docs, skills, SETUP.md, or rhidoc
  workspace files were changed. SKILL.md §2 still documents only OpenAlex config; the
  rhidoc providers index (doc02.06.01.00) still says "One provider today: OpenAlex";
  there is no `semanticscholar` rhidoc doc yet. The CLI surface and config key above are
  the stable contract Phase 2 documents.
