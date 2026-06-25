# Provider refactor — Phase 3: arXiv provider (first trait-native provider)

## Motivation

Fast-moving CS/ML literature (e.g. constrained/structured LLM decoding) lives on
**arXiv** as preprints that OpenAlex under-indexes and often stores without abstracts.
The keyless Semantic Scholar path is rate-limited and depends on S2 having ingested the
preprint. There is no first-class way to seed and snowball from arXiv, so for current
LLM-research questions the coverage funnel starts broken and nothing accumulates into
the Library/Catalog.

This phase adds a `braincrawl arxiv` provider: search arXiv by query and fetch by arXiv
id, pushing works + **real abstracts** (arXiv always has them) into the shared
Library/Catalog so coverage compounds — merging onto existing nodes by arXiv id and DOI
rather than forking duplicates.

It is also the **validation case** for the Phase 1–2 provider refactor: arXiv is the
most-different provider we have (Atom XML not JSON, abstract-rich, no citation graph),
so implementing it against the `Provider` trait proves the abstraction isn't secretly
OpenAlex-shaped. If the trait leaks, this is where it shows.

## Do NOT

- Do NOT implement `cited-by` or `refs` for arXiv — arXiv's API has no citation graph.
  arXiv supports only `Get` and `Search`; `dispatch` returns
  `ProviderError::UnsupportedVerb` for everything else.
- Do NOT add `chrono`/`atom_syndication`/`feed-rs` or a second HTTP stack. Use the
  existing `reqwest::blocking` pattern and add ONLY `quick-xml` (with its `serde`
  feature) for parsing.
- Do NOT change the Phase 1/2 contract types (`Emission`, `ProviderCmd`, `Provider`).
  Implement against them as they exist after Phase 2 (see "Surface from Phase 2" below).
- Do NOT create a new crate. arXiv is a module under `apps/cli/src/arxiv/`.
- Do NOT persist version-suffixed arXiv ids as the canonical alias — strip the trailing
  `vN` so `arxiv:2301.07041` (not `arxiv:2301.07041v2`) is stored, matching how S2's
  `extract_aliases` emits the `arxiv` namespace. This is the dedup contract.
- Do NOT invent new NodeKinds or relations — arXiv records map to `kind = "Work"`,
  same shared catalog vocabulary as every other provider.
- Do NOT change the server or store. Abstracts ride inside `attrs` (an `abstract`
  string field), surfaced by `--abstract`, exactly as Semantic Scholar does today.

## Surface from Phase 2 (implement against this)

- `provider::Provider { fn name() -> &'static str; fn dispatch(&self, ProviderCmd, &OutputOpts) -> Result<(Envelope, Emission), ProviderError> }`
- `provider::ProviderCmd` union enum with `Get { id }` and
  `Search { entity: Option<String>, query }` (arXiv ignores `entity`, expecting `None`).
- `provider::{Emission, WorkRecord, Alias, EdgeInput, PushSummary}` and shared
  `rfc3339_now`. arXiv produces records only; `Emission.edges` is always empty.
- `provider::ProviderError::UnsupportedVerb { provider, verb }`.
- `main.rs` dispatches via `run_provider`; a new provider needs (1) a module impl, (2) a
  clap enum mapping into `ProviderCmd`, (3) one dispatch arm.

## Plan

### 1. Add the `quick-xml` dependency

In `apps/cli/Cargo.toml`, add `quick-xml = { version = "0.36", features = ["serialize"] }`
(pin to the workspace's resolver; pick the current 0.x that builds). This is the only
new dependency.

### 2. `arxiv/client.rs` — HTTP client returning raw XML

Mirror `semanticscholar/client.rs` (reqwest::blocking, `braincrawl/{version}`
user-agent, exponential-backoff retry on 429/5xx), but the arXiv API returns Atom XML,
so methods return the raw body `String`, not JSON.

- Base URL: `http://export.arxiv.org/api/query`.
- `search(&self, search_query: &str, start: u32, max_results: u32) -> Result<String>`
  → `?search_query=<q>&start=<n>&max_results=<m>`.
- `get(&self, id_list: &str) -> Result<String>` → `?id_list=<ids>`.
- `with_base_url` test hook, like S2.
- **Rate limit:** arXiv requests ≥3s between calls. Sleep ~3s between paginated search
  pages (stricter than other providers' backoff). A single `get`/first page needs no
  pre-sleep.

### 3. `arxiv/parse.rs` — Atom XML → `Vec<serde_json::Value>`

The novel part. Deserialize the Atom feed with `quick-xml`'s serde support into typed
structs, then convert each `<entry>` into a normalized `serde_json::Value` (so the rest
of the pipeline is identical to JSON providers). Per entry, extract:

- `arxiv_id`: from `<id>` (`http://arxiv.org/abs/2301.07041v2`) — strip the URL prefix
  AND the trailing `vN` → `2301.07041`. Handle old-style ids (`hep-th/9901001`) too.
- `title`: `<title>` (collapse internal whitespace/newlines).
- `abstract`: `<summary>` (trimmed).
- `authors`: list of `<author><name>`.
- `published` / `updated`: the Atom timestamps.
- `doi`: the `<arxiv:doi>` extension element when present (bare).
- `primary_category`: `<arxiv:primary_category term="…">`.
- `categories`: `<category term="…">` list.
- `pdf_url`: the `<link title="pdf" href="…">`.

Return `Vec<Value>` (one object per entry) plus the feed's `<opensearch:totalResults>`
if present (for the envelope `count`). quick-xml + namespaced elements (`arxiv:`,
`opensearch:`) can be fiddly — if serde-deriving the extension namespaces is painful,
a small manual `quick-xml` reader over events is acceptable; keep it confined to this
file. Add unit tests over a committed fixture (step 7).

### 4. `arxiv/entity.rs` — id inference

arXiv is works-only, so no entity enum is needed (or a trivial single-variant one for
symmetry — keep it minimal). Provide `infer_id(id: &str) -> Result<String>` returning
the bare, version-stripped arXiv id accepted by `id_list`:

- `arxiv:2301.07041` / `arxiv:2301.07041v2` → `2301.07041`
- bare `2301.07041` / `2301.07041v2` → `2301.07041`
- old-style `hep-th/9901001` (and `arxiv:hep-th/9901001`) → `hep-th/9901001`
- full URL `http(s)://arxiv.org/abs/2301.07041v2` → `2301.07041`
- otherwise a loud error naming the bad id and accepted forms.

Unit-test each form, especially version stripping and old-style ids.

### 5. `arxiv/mapping.rs` — lower to `Emission`

- `node_kind` → always `"Work"`.
- `extract_aliases(record: &Value) -> Vec<Alias>`:
  - `arxiv:<bare-versionless-id>` (the cross-provider merge key with S2's `arxiv`).
  - `doi:<bare>` when the entry reported `<arxiv:doi>` (the merge key with
    OpenAlex/Crossref's published version). This is the dedup guarantee from the
    original request.
- `to_emission(records: &[Value]) -> Emission`: build `WorkRecord { source: "arxiv",
  kind: "Work", aliases, attrs: record }` for each; `edges` always empty;
  `skipped_unmappable` always 0.

Test: an entry with a `<arxiv:doi>` yields both `arxiv:` and `doi:` aliases, version
stripped; an entry without a DOI yields only the `arxiv:` alias.

### 6. `arxiv/shape.rs` + `arxiv/verbs.rs` + `arxiv/mod.rs`

- `shape.rs`: curated-field trim mirroring S2 — keep `arxiv_id, title, abstract,
  authors, published, doi, primary_category, categories, pdf_url`; **drop `abstract`
  unless `opts.include_abstract`** (same `--abstract` rule as S2). `build_envelope`
  like S2's.
- `verbs.rs`:
  - `get(client, id, opts) -> Result<(Envelope, Emission)>`: `infer_id`, `client.get`,
    `parse`, build envelope + emission.
  - `search(client, query, opts) -> Result<(Envelope, Emission)>`: paginate with
    `start`/`max_results` honoring `--limit`/`--all` (sleep ≥3s between pages). Query
    handling: if `query` already contains an arXiv field operator
    (`ti:`/`au:`/`abs:`/`cat:`/`all:`/`co:`/`jr:`), pass it verbatim as `search_query`;
    otherwise wrap as `all:<query>`.
- `mod.rs`: `ArxivError` (`Http`/`Api`/`InferFailed`/`Parse`), `Result`, and
  `pub struct ArxivProvider` with `impl Provider`:
  - `name()` → `"arxiv"`.
  - `dispatch`: `ProviderCmd::Get { id }` → `verbs::get`;
    `ProviderCmd::Search { query, .. }` → `verbs::search` (ignore `entity`);
    all other variants → `ProviderError::UnsupportedVerb { provider: "arxiv", verb }`.
  - `ArxivError` converts into `ProviderError::Other`.

### 7. Wire it up + fixtures

- `apps/cli/src/lib.rs`: `pub mod arxiv;`.
- `apps/cli/src/cli.rs`: add `Arxiv(ArxivArgs)` to `Namespace`, `ArxivArgs { cmd: ArxivCmd }`,
  and `ArxivCmd { Get { id }, Search { query } }`. **No `entity` positional** (arXiv is
  works-only) — `braincrawl arxiv search "constrained decoding"`.
- `apps/cli/src/main.rs`: a `Namespace::Arxiv` arm that constructs `ArxivProvider`, maps
  `ArxivCmd` → `ProviderCmd` (`Search { query }` → `Search { entity: None, query }`),
  and calls `run_provider` (the Phase 2 helper).
- Commit a sample arXiv Atom XML response at
  `apps/cli/tests/fixtures/arxiv_search.xml` (multi-entry, including one entry with
  `<arxiv:doi>` and one without, and a versioned `<id>`). Use it in `parse`/`mapping`
  tests.

## Files to Modify

- `apps/cli/Cargo.toml` — add `quick-xml`.
- `apps/cli/src/lib.rs` — `pub mod arxiv;`.
- `apps/cli/src/cli.rs` — `Arxiv` namespace + `ArxivCmd`.
- `apps/cli/src/main.rs` — `Namespace::Arxiv` dispatch via `run_provider`.
- `apps/cli/src/arxiv/mod.rs` — NEW: error type, `ArxivProvider`, `impl Provider`.
- `apps/cli/src/arxiv/client.rs` — NEW.
- `apps/cli/src/arxiv/parse.rs` — NEW (XML → Value, tested against fixture).
- `apps/cli/src/arxiv/entity.rs` — NEW (id inference, tested).
- `apps/cli/src/arxiv/mapping.rs` — NEW (aliases + `to_emission`, tested).
- `apps/cli/src/arxiv/shape.rs` — NEW (trim + envelope).
- `apps/cli/src/arxiv/verbs.rs` — NEW (`get`, `search`).
- `apps/cli/tests/fixtures/arxiv_search.xml` — NEW fixture.

## Verification

```bash
cargo build --bin braincrawl --bin braincrawl-server
cargo test -p braincrawl-cli
cargo clippy -p braincrawl-cli --no-deps 2>&1 | grep -i "warning\|error" || echo "clippy clean"
```

Parse/mapping/entity tests must pass offline against the committed fixture (no network
in tests). Manual smoke (needs network + running server):
`braincrawl arxiv search "constrained decoding" --limit 5 --abstract` returns entries
with abstracts and pushes Works; `braincrawl arxiv get arxiv:2301.07041` resolves and
merges by `arxiv:`/`doi:` alias (re-running does not fork a duplicate node).

## Out of Scope

- Citation edges / snowballing from arXiv (no citation graph; OpenAlex/S2/OpenCitations
  remain the edge sources).
- Fulltext ingestion beyond the existing `fetch-content`/`fetch-pdf` verbs (arXiv PDFs
  can be fetched later via those, keyed on the stored work).
- PhilPapers or any other provider (separate request).
- Subprocess plugin loader and the providers crate split (both deferred).

## Notes

- The dedup/merge correctness is the subtle bit: `arxiv:` must be version-stripped to
  match S2, and `doi:` must be emitted when arXiv reports the published DOI — that's how
  an arXiv preprint and its published version collapse to one canonical node via the
  Catalog's union-find.
- arXiv's `search_query=all:` is noisy; the field-operator passthrough lets power users
  scope with `ti:`/`cat:` etc. Document the default-`all:` behavior in the arXiv doc.
- This phase also discharges `feedback-arxiv-provider` (the original feedback request).

## Surface after this phase

- `braincrawl arxiv get <id>` and `braincrawl arxiv search <query>` exist, push Works
  with abstracts into the Catalog by default (`--skip-push` opts out), and merge by
  `arxiv:`/`doi:` alias.
- `arxiv::ArxivProvider` implements `provider::Provider` (`name() == "arxiv"`; supports
  `Get`/`Search`; `UnsupportedVerb` otherwise) — demonstrating the trait holds for a
  non-JSON, edge-less provider.
- `quick-xml` is a CLI dependency; `arxiv/parse.rs` is the only XML-aware module.
- Negative space: arXiv emits no edges; no new NodeKinds/relations; no new crate; no
  subprocess loader. The Phase 1/2 contract types are unchanged.
