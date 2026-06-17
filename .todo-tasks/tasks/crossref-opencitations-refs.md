# Backfill backward-reference edges via Crossref + OpenCitations

## Motivation

OpenAlex's MAG-derived records for older works often carry **zero `referenced_works`** —
`braincrawl openalex refs <id>` returned empty for landmark works (W99391817,
W94828380). Forward citations are rich; the backward direction is the gap. Crossref and
OpenCitations are open, DOI-keyed reference sources that backfill the missing backward
edges without paying for a full second metadata provider. This is a **narrow L2
backward-edge backfill**, not a metadata provider.

## Do NOT

- Do NOT push node *records* for cited works. `put_edges` auto-creates stub nodes for
  unknown alias endpoints (`resolve_or_mint_stub` in `crates/core/src/usecases.rs:280`),
  so this task pushes **edges only**. Pushing minimal cited-work metadata is out of scope.
- Do NOT reuse the OpenAlex `to_edges` (it hardcodes the `openalex` namespace). These
  edges are `doi`-namespaced on both endpoints — write a small dedicated edge mapper.
- Do NOT fetch forward citations (OpenAlex cited-by already covers forward well).
- Do NOT attempt to resolve DOI-less / unstructured references. Skip them, count them,
  report the count. No fuzzy title matching.
- Do NOT touch the OpenAlex or Semantic Scholar provider modules. This is additive.
- Do NOT add a combined `refs --source` verb. Two separate provider namespaces now; a
  combining verb is a deliberate later task.

## Plan

Mirror the established provider pattern (see `apps/cli/src/semanticscholar/` — module with
`client`, `verbs`, `mapping`, registered in `apps/cli/src/lib.rs`, dispatched in
`apps/cli/src/main.rs`, namespace in `apps/cli/src/cli.rs`). Two new provider namespaces
share one new module tree.

### 1. New module(s) under `apps/cli/src/`
Create a module tree (e.g. `refs_backfill/` with `crossref.rs`, `opencitations.rs`, and a
shared `mapping.rs`), registered with `pub mod …;` in `apps/cli/src/lib.rs` next to the
existing provider modules. Shared logic:

- **Resolve the citing work's DOI:** `store_client.get_work(id)` returns the work JSON
  including an `aliases` array of `{namespace, value}`. Find the entry with
  `namespace == "doi"` → that's the citing DOI. If none, return a clean error
  ("no DOI known for <id> — Crossref/OpenCitations are DOI-keyed"). `id` may arrive in any
  `ns:value` form; pass it through to `get_work` as-is.
- **doi-namespaced edge mapper:** given (citing_doi, [cited_doi]) and a `source` string,
  build edge JSON values shaped exactly like the existing edge contract:
  `{"src":{"namespace":"doi","value":citing_doi},"dst":{"namespace":"doi","value":cited_doi},"relation":"cites","source":<source>,"attrs":null,"fetched_at":<rfc3339-now>}`.
  Reuse the RFC3339-now helper style already in `openalex/mapping.rs`.
- **Push:** call `store_client.put_edges(&edge_values)` (existing method). Honor
  `--skip-push` (don't push; still render the discovered DOIs). Report a push summary line
  consistent with the other namespaces (edges stored; references skipped for missing DOI).

### 2. Crossref source — `crossref.rs`
- `GET https://api.crossref.org/works/{doi}` (URL-encode the DOI path segment). Polite
  pool: append `?mailto={email}` when configured, and send a descriptive User-Agent
  including the mailto.
- Parse `message.reference` (array). For each entry, take `DOI` (lowercase it; Crossref
  DOIs are case-insensitive — normalize to lowercase to match stored `doi:` aliases).
  Entries with no `DOI` are skipped and counted.
- Return the cited DOIs + skipped count.

### 3. OpenCitations source — `opencitations.rs`
- `GET https://opencitations.net/index/coci/api/v1/references/{doi}` (no key). Each array
  element has a `cited` field containing the cited DOI (lowercase to normalize).
- Return the cited DOIs.

### 4. CLI surface — `cli.rs`
Add two `Namespace` variants (place after `Semanticscholar`, before `Graph`):
```
#[command(about = "Backfill backward references from Crossref's deposited reference list")]
Crossref(CrossrefArgs),
#[command(about = "Backfill backward references from the OpenCitations COCI index")]
Opencitations(OpencitationsArgs),
```
Each with a single `refs` subcommand carrying the work id, mirroring `OpenalexCmd::Refs`:
```
#[derive(Subcommand)] pub enum CrossrefCmd { /// List works the given work references
    Refs { /// Work id in ns:value form (resolved to its DOI) id: String } }
```
…and an analogous `OpencitationsCmd`.

### 5. Dispatch — `main.rs`
Add `Namespace::Crossref(c)` and `Namespace::Opencitations(o)` arms. Build a `StoreClient`
(with auth token, like the other arms), resolve the DOI, call the source, push edges unless
`--skip_push`, render the discovered cited DOIs through the existing `Envelope`/`render`
path (one result per cited DOI, e.g. `{"id":"doi:<cited>"}`), and report the push summary.

### 6. Config — `config.rs`
Add `crossref_mailto: Option<String>` to `Config` and `ConfigFile`, resolved in
`Config::resolve()` as
`std::env::var("BRAINCRAWL_CROSSREF_MAILTO").ok().or_else(|| file.crossref_mailto)`.
OpenCitations needs no config.

## Files to Modify

- `apps/cli/src/refs_backfill/…` — NEW module(s): crossref client, opencitations client,
  shared DOI-edge mapper + DOI resolution.
- `apps/cli/src/lib.rs` — register the new module(s).
- `apps/cli/src/cli.rs` — `Crossref`/`Opencitations` namespaces + their `refs` subcommands.
- `apps/cli/src/main.rs` — two dispatch arms.
- `apps/cli/src/config.rs` — `crossref_mailto`.
- Inline `#[cfg(test)] mod tests` alongside the new code.

## Verification

```bash
cargo test -p braincrawl-cli --lib
cargo build --release --bin braincrawl
# Unit tests (no live HTTP — split parsing/mapping into pure functions) must cover:
# (a) parsing a Crossref `message.reference` array → cited DOIs, skipping DOI-less entries
#     and returning the skipped count;
# (b) parsing an OpenCitations references response → cited DOIs;
# (c) the doi-edge mapper builds src/dst with namespace "doi", relation "cites", correct source;
# (d) DOI extraction from a get_work JSON `aliases` array (present → DOI; absent → error).
```

## Out of Scope

- Forward citations from these sources.
- DOI-less / unstructured reference resolution (fuzzy title matching).
- Pushing cited-work metadata/nodes (edges-only; stubs auto-mint).
- A combined `refs --source crossref|opencitations|both` verb (deliberate later task —
  it can sit on top of these two namespaces).
- Semantic Scholar (already landed).

## Notes

- Edges-only works because `put_edges` auto-mints endpoint stubs and OpenAlex-pushed works
  already carry a `doi:` alias — so the citing endpoint lands on the existing canonical node
  and any already-known cited DOI merges rather than forking. Normalize DOIs to lowercase so
  they match the stored `doi:` alias form.
- Expected dead-end: pre-DOI humanities landmarks have no DOI and thus can't be backfilled —
  surface that as a clean per-work error, don't try to solve it here.
- Keep HTTP thin; keep parsing/mapping in pure functions so the verification gate runs
  offline. Match provider conventions: blocking reqwest, `ClientError` mapping, RFC3339 helper.
- Later doc pass: braincrawl skill §3 ("backward refs when they exist") should mention these
  two backfill namespaces.

## Surface after this phase

- Two CLI namespaces: `braincrawl crossref refs <id>` and
  `braincrawl opencitations refs <id>`, each resolving the work's DOI, fetching cited DOIs,
  and pushing `doi`-namespaced `cites` edges (source `crossref` / `opencitations`) unless
  `--skip-push`.
- A shared DOI-edge mapper + `get_work`-based DOI resolution helper in the new module.
- `Config.crossref_mailto` (env `BRAINCRAWL_CROSSREF_MAILTO` > config.toml `crossref_mailto`).
- Unchanged and still relied upon: OpenAlex/Semantic Scholar provider modules, the existing
  `to_edges`/`to_work_record` openalex push path, `store_client.put_edges`/`get_work`, and
  the server edge/stub-minting behavior.
