# Agent Result: braincrawl-openalex-push

date: 2026-06-16T22:03:01-04:00
session: completed
verification: passed
commits: 1
branch: chain-braincrawl-openalex_claude_braincrawl-openalex-push
surface deviations: declared
session id: ffe54d6d-c664-4a52-b787-8210bd37911c


## Summary

All done. Here's a summary of what was implemented:

---

## Implementation Summary

Implemented the full `braincrawl-openalex-push` plan across 7 files in one commit.

### Step 0 — Auth wiring
- **`config.rs`**: Added `auth_token: Option<String>` resolved from `BRAINCRAWL_AUTH_TOKEN` env (> config file), mirroring the existing field precedence pattern.
- **`store_client.rs`**: Added `token: Option<String>` field, `with_token(Option<String>) -> Self` builder, and `apply_auth()` helper that attaches `.bearer_auth(token)` to all four HTTP methods (`put_work`, `put_edges`, `have`, `get_work`).
- **`main.rs`**: Both the `Store` and `Openalex` arms now build `StoreClient::new(...).with_token(config.auth_token.clone())`.

### Step 1 — Mapping module (`openalex/mapping.rs`)
- `node_kind(Entity)`: works→`"Work"`, authors→`"Author"`, sources→`"Venue"`, topics→`"Topic"`, concepts→`"Concept"`, others→`None`.
- `extract_aliases(Entity, &Value)`: entity-aware alias extraction with URL prefix stripping (`https://openalex.org/`, `https://doi.org/`, `https://orcid.org/`, `https://www.wikidata.org/...`). Works extracts openalex/doi/pmid/pmcid/mag; Authors: openalex/orcid; Sources: openalex/issn_l/issn[]; Topics/Concepts: openalex (+ wikidata for Concepts).
- `to_work_record(Entity, &Value)`: wraps into `{source,kind,aliases,attrs}`, returns `None` for unmappable entities.

### Step 2 — Edge mapping
- `to_edges(&[(String,String)])`: builds `EdgeInput` JSON with `{src,dst,relation:"cites",source:"openalex",attrs:null,fetched_at}` using a pure-`std` RFC3339 timestamp. URL prefixes stripped from both IDs.

### Step 3 — Push wiring (`main.rs`)
- After `render(&envelope)`, calls `push_batch_to_store` if `!opts.skip_push`.
- `push_batch_to_store`: iterates records, calls `to_work_record` + `store.put_work`; skips `None` entities; calls `to_edges` + `store.put_edges` for non-empty edge batches.
- `report_push_summary`: prints counts and errors to stderr. Exit code 1 if any push errors.

### Bug fix in `verbs.rs`
- Fixed `cited_by`'s `cited_id = citing_entity.to_string() + ":" + &path_id` (produced `"works:W1234"`) → now stores bare `path_id` (`"W1234"`), which `to_edges` handles correctly.

### Integration test (`tests/openalex_push.rs`)
- 10 tests: node_kind variants, alias URL normalization for works/authors/sources, institution non-pushability, `to_edges` shape, auth-enabled server round-trip (`put_work` + `get_work`), 401/403 on missing/wrong token, edge push count.

All 81 tests pass (`cargo build -p braincrawl-cli` + `cargo test -p braincrawl-cli`).

---

## Notes

- The `rfc3339_now()` timestamp helper is pure-std (no chrono dependency). The date arithmetic is straightforward Gregorian — tested implicitly via `to_edges` tests checking `fetched_at` is a non-null string.
- The `cited_by` edge bug fix was a necessary correction: the old code stored `"works:W1234"` as the cited ID, which would have produced edges with `dst.value = "works:W1234"` rather than the bare `"W1234"` the server expects.
- `push_batch_to_store` is in `main.rs` rather than `verbs.rs` — the plan allowed either, and this keeps verbs.rs clean with unchanged signatures (still returns `(Envelope, PushBatch)`).
- Non-pushable entities (Institutions, Publishers, Funders, Keywords) increment `skipped_unmappable` and are silently omitted from push, as specified.

## Surface Deviations

None. All declared surface items are present:
- `Config.auth_token` (`BRAINCRAWL_AUTH_TOKEN`) ✓
- `StoreClient` sends `Authorization: Bearer <token>` on every request ✓
- `apps/cli/src/openalex/mapping.rs`: `node_kind`, `extract_aliases`, `to_work_record`, `to_edges` ✓
- Verbs push by default; `--skip-push` opts out; unmappable entities are context-only ✓
- Push summary on stderr; stdout remains the clean `Envelope` ✓
- Negative space (non-pushable kinds, content-payload upload, identity resolution) unimplemented ✓

## Commits

```
0fdf572 feat: braincrawl-openalex-push (agent)
```

## Build & Test Output (last 30 lines)

```
test infer_authors_orcid_url ... ok
test filter_unknown_key_is_loud_error ... ok
test infer_concepts_openalex_id ... ok
test infer_funders_openalex_id ... ok
test infer_institutions_openalex_id ... ok
test infer_institutions_ror ... ok
test infer_openalex_url_strips_prefix ... ok
test infer_publishers_openalex_id ... ok
test infer_sources_issn ... ok
test infer_sources_openalex_id ... ok
test infer_keyword_slug ... ok
test infer_topics_openalex_id ... ok
test infer_unknown_returns_err ... ok
test infer_works_doi_prefix ... ok
test infer_works_doi_url ... ok
test infer_works_mag ... ok
test infer_works_openalex_id ... ok
test infer_works_pmcid ... ok
test infer_works_pmid ... ok
test trim_works_authorships_simplified ... ok
test trim_works_full_flag_bypasses_trimming ... ok
test trim_works_keeps_curated_and_drops_bulk_fields ... ok

test result: ok. 34 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests braincrawl_cli

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```
