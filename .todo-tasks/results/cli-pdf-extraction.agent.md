# Agent Result: cli-pdf-extraction

date: 2026-06-18T11:05:53-04:00
session: completed
verification: passed
commits: 1
branch: feat/milestone01_claude_cli-pdf-extraction
surface deviations: none
turns: 64/100
cost: $2.0429435/$5.00
uncommitted: none
session id: b3fe346c-0d67-4f07-9a99-cfb1e2087428


## Summary

None.

## Commits

```
8722690 feat: extract-text CLI verb via pdf-extract
```

## Build & Test Output (last 30 lines)

```
test envelope_carries_count_and_resolved_filter ... ok
test envelope_limit_truncates ... ok
test infer_arxiv_prefix ... ok
test infer_bare_40hex_maps_to_papers ... ok
test infer_corpusid_prefix ... ok
test infer_doi_prefix_maps_to_papers ... ok
test infer_mag_prefix ... ok
test infer_native_doi_passes_through ... ok
test infer_pmcid_prefix ... ok
test infer_pmid_prefix ... ok
test infer_s2_prefix_papers ... ok
test infer_s2author_prefix ... ok
test infer_unknown_form_fails ... ok
test node_kind_authors_is_author ... ok
test node_kind_papers_is_work ... ok
test trim_full_bypasses_trimming ... ok
test trim_paper_includes_abstract_with_flag ... ok
test trim_paper_keeps_curated_drops_abstract_by_default ... ok
test work_record_author_has_semanticscholar_source ... ok
test work_record_paper_has_semanticscholar_source ... ok

test result: ok. 25 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests braincrawl_cli

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.13s
```
