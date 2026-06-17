# Agent Result: braincrawl-openalex-read

date: 2026-06-16T21:54:38-04:00
session: completed
verification: passed
commits: 1
branch: chain-braincrawl-openalex_claude_braincrawl-openalex-read
surface deviations: declared
session id: c93b7c79-0f8c-47d7-bb7b-a1dd53130561


## Summary

Implementation complete. 65 tests passing, commit made.

---

## Implementation Summary

Added the full `braincrawl openalex` read-only vocabulary to `apps/cli`. The implementation follows the plan step-by-step:

**New module `apps/cli/src/openalex/`:**
- **`entity.rs`**: `Entity` enum (9 variants) + `infer_entity()` covering all ID prefix forms (W/A/S/I/T/P/F/C prefixes, doi:/orcid:/issn:/ror: schemes, full OpenAlex URLs, `keywords/<slug>`)
- **`filters.rs`**: `KV` struct, `validate_filters()` backed by static allowlists for all 9 entities — bad keys produce a loud error naming the offender
- **`client.rs`**: `OpenAlexClient` with injectable `base_url` (for test fixtures), optional `api_key`, cursor-paging `list()`, `get_one()`, `autocomplete()`, and 429/5xx backoff retry (1s/2s/4s)
- **`shape.rs`**: `trim()` with per-entity curated field sets, authorship simplification, `reconstruct_abstract()` from inverted index, `build_envelope()` with `--full` bypass and `--abstract` injection
- **`verbs.rs`**: Six verbs; `refs` batch-fetches referenced_works in ≤50-ID chunks via `ids.openalex:` OR filter; `collect_pages` handles `--all` cursor drain vs single-page `--limit`
- **`mod.rs`**: `OpenAlexError`, `PushBatch { records, edges }` (returned by all verbs, discarded this phase)

**Modified files:**
- `cli.rs`: `Openalex` namespace + 6 verb subcommands; `--abstract` flag; all `GlobalArgs` fields marked `global = true` so they're accepted after subcommand names
- `main.rs`: dispatches openalex verbs, discards `PushBatch`
- `lib.rs`: exposes `openalex` module

**Tests:** 34 fixture-based integration tests + 28 inline unit tests; 3 existing foundation tests still pass.

## Notes

- `GlobalArgs` fields were marked `global = true` — not in the original plan but required to make the plan's own verification command work (`--limit` after the subcommand name).
- `KV::parse()` splits on the first colon, so values containing colons (URLs, range operators like `>100`) parse correctly.
- The `refs` verb fetches `referenced_works` IDs from the work object first, then batch-fetches in ≤50-ID chunks; this may issue multiple round-trips for a heavily-cited work.
- `infer_entity` cannot infer Keywords from a bare slug (e.g., `computer-science`) — user must prefix `keywords/computer-science`. This matches the API's own ID form.

## Surface Deviations

None. All declared surface items are present with the specified signatures:
- `Entity` enum + `infer_entity(&str) -> Result<(Entity, String)>` ✓
- `validate_filters(Entity, &[KV]) -> Result<String>` ✓  
- `OpenAlexClient` (injectable `base_url`, `api_key`, 429 backoff, cursor paging) ✓
- `trim(Entity, &Value) -> Value`, `reconstruct_abstract` ✓
- `PushBatch { records: Vec<(Entity, Value)>, edges: Vec<(String, String)> }` ✓
- Nothing written to store; `StoreClient` not called from openalex; `--skip-push` parsed but no-op ✓

## Commits

```
dee54b2 feat: braincrawl openalex read vocabulary (context-only)
```

## Build & Test Output (last 30 lines)

```
test trim_works_keeps_curated_and_drops_bulk_fields ... ok

test result: ok. 34 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests braincrawl_cli

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.10s
     Running `target/debug/braincrawl openalex find works 'publication_year:2020' 'is_oa:true' --limit 1 --help`
Filter an entity collection by key:value expressions

Usage: braincrawl openalex find [OPTIONS] <ENTITY> [FILTERS]...

Arguments:
  <ENTITY>      Entity type (works, authors, sources, institutions, topics, keywords, publishers, funders)
  [FILTERS]...  Filter expressions in `key:value` form (e.g. `publication_year:2020` `is_oa:true`)

Options:
      --json             Output as JSON (default)
      --text             Output as compact text (one result per line)
      --limit <LIMIT>    Maximum number of results to return
      --all              Return all results, ignoring limit
      --fields <FIELDS>  Comma-separated list of fields to include in output
      --full             Return full record (all fields)
      --skip-push        Do not push results to the local store
      --abstract         Include reconstructed abstract text (bulky; off by default)
  -h, --help             Print help
```
