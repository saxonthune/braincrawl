# Agent Result: oa-fulltext-acquisition

date: 2026-06-17T14:41:35-04:00
session: completed
verification: passed
commits: 1
branch: feat/milestone01_claude_oa-fulltext-acquisition
surface deviations: none
turns: 27/100
cost: $1.5050476499999998/$5.00
uncommitted: none
session id: 7fb1ac5e-e2fe-46ae-b29d-f5f877f5ad0b


## Summary

None.

## Commits

```
136b3fc feat: oa-fulltext acquisition — fetch-content CLI command
```

## Build & Test Output (last 30 lines)

```
test semanticscholar::entity::tests::infer_s2author ... ok
test semanticscholar::entity::tests::infer_unknown_fails_loudly ... ok
test semanticscholar::mapping::tests::aliases_author_orcid_url_stripped ... ok
test semanticscholar::mapping::tests::aliases_paper_corpusid_numeric ... ok
test semanticscholar::mapping::tests::aliases_author_with_orcid ... ok
test semanticscholar::mapping::tests::aliases_paper_doi_bare_merge_guarantee ... ok
test semanticscholar::mapping::tests::node_kind_authors_to_author ... ok
test semanticscholar::mapping::tests::aliases_paper_doi_url_stripped ... ok
test semanticscholar::entity::tests::infer_arxiv ... ok
test semanticscholar::entity::tests::infer_ambiguous_bare_numeric_fails ... ok
test semanticscholar::entity::tests::infer_s2_native_arxiv ... ok
test semanticscholar::entity::tests::infer_s2_native_doi ... ok
test semanticscholar::entity::tests::infer_s2_native_mag ... ok
test semanticscholar::mapping::tests::node_kind_papers_to_work ... ok
test semanticscholar::mapping::tests::to_work_record_author_shape ... ok
test semanticscholar::mapping::tests::to_edges_shape_and_source ... ok
test semanticscholar::mapping::tests::to_work_record_paper_shape ... ok
test semanticscholar::shape::tests::envelope_carries_count_and_filter ... ok
test semanticscholar::shape::tests::envelope_limit_truncates_results ... ok
test semanticscholar::shape::tests::trim_full_bypasses_trimming ... ok
test semanticscholar::shape::tests::trim_author_keeps_curated_fields ... ok
test semanticscholar::shape::tests::trim_paper_includes_abstract_with_flag ... ok
test semanticscholar::shape::tests::trim_paper_keeps_curated_fields ... ok
test store_client::tests::put_params_required_fields ... ok
test store_client::tests::put_params_link_only_rights ... ok
test store_client::tests::put_params_optional_fields_present ... ok

test result: ok. 86 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

    Finished `release` profile [optimized] target(s) in 0.18s
```
