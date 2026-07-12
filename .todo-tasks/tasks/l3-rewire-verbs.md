# Rewire existing l3 verbs onto the new parser

## Motivation

`l3 list`, `l3 check`, and `l3 index` parse the old doc format with ad-hoc
code in `apps/cli/src/l3.rs`. Once crate `l3` owns parsing there must be one
parser in the codebase, and the agent-facing contract must describe the node
grammar so future sessions write the new format.

## Do NOT

- Do NOT change any verb's CLI signature — flags and arguments stay exactly
  as they are (`new`, `path`, `list`, `check`, `index`, `import`, `rm`,
  plus `assign-ids` from phase 2).
- Do NOT break on old-format docs: they still list, index (with whatever the
  parser lifts), and check (with warnings naming what's unmigrated). The
  store currently contains old-format docs and migration is a separate,
  interactive effort.
- Do NOT use the word `refutes` anywhere — the claim-link word is
  `contradicts` (decided 2026-07-12).
- Do NOT restate the grammar in SKILL.md — it points to
  `l3-conventions.md`, which is the single source of truth for the contract.
- Do NOT leave deprecation/changelog narration in the docs — they state
  current truth only.

## Plan

### 1. Reimplement the verbs over `l3::parse()`

In `apps/cli/src/l3.rs`:
- `cmd_list`: doc rows derived from the graph's per-doc envelope (doc,
  schema, modified, title from the doc's first node or H1) — same output
  columns as today.
- `cmd_check`: envelope lint as today, plus the parser's warnings for the
  checked doc(s) (anchor-less nodes, malformed lines).
- `cmd_index` / `reindex`: regenerate `INDEX.md` from the graph — the same
  three sections (Documents, Cross-references, Work index), now derived:
  cross-references from links between docs (any link whose endpoints span
  two docs, plus doc-level forward references), work index from Catalog
  endpoints. Keep the exact table formats so downstream readers don't
  change.
- Delete the superseded ad-hoc harvesting (`harvest_links`, `harvest_ids`,
  `read_meta`'s body parsing) once nothing calls it; keep the
  frontmatter/envelope helpers the import/new paths still use.

### 2. Scaffolds emit the node grammar

`scaffold()` in `apps/cli/src/l3.rs`: replace `SPINE_BODY`/`DIALECTICAL_BODY`
with node-grammar bodies — an "About this document" node plus one example
question node, property bullets (`- tags:`, `- remarks:`), one commented
link-line example using `contradicts` and `catalog`. Headings carry NO
anchors (assign-ids mints them); note that in the scaffold comment.

### 3. Update the agent-facing contract

- `.claude/skills/braincrawl/l3-conventions.md`: the settled contract
  becomes the node grammar — node = `##` heading + tooling-owned `^r-…`
  anchor; property lines (`- key: value`, one optional `{flow map}` per
  line, values with commas quoted); link lines (`- kind [[target]]`,
  `- [[src]] kind [[dst]]`, trailing `{props}`); `- tags: #…` = labels;
  title is opaque prose; `catalog` is the work-link convention;
  `contradicts`/`supports`/`builds-on` are free vocabulary with
  `contradicts` as the blessed claim word; `reading: {role, why}` marks
  recommended reading; anchors and INDEX.md are tooling-owned — agents
  never write them. Keep the settled-contract / experimental-ledger split
  intact; move superseded body conventions out rather than annotating them.
- `.claude/skills/braincrawl/SKILL.md` §5/CLI surface: mention `assign-ids`
  in the verb list and that docs are collections of research nodes; keep it
  a pointer to l3-conventions.md for the grammar.

## Files to Modify

- `apps/cli/src/l3.rs` — verbs over the parser; scaffolds; dead code removal
- `apps/cli/src/cli.rs` — only if doc-comments there describe the old bodies
- `.claude/skills/braincrawl/l3-conventions.md` — the node-grammar contract
- `.claude/skills/braincrawl/SKILL.md` — verb list + pointer updates

## Verification

```bash
cargo build
cargo test -p braincrawl-cli
cargo test
```

## Out of Scope

- Migrating the existing docs' content (interactive session, separate)
- New verbs, the reading-list verb, any server change

## Notes

- Existing tests in `apps/cli/src/l3.rs` that pin old harvesting behavior
  will need updating to the graph-derived equivalents — update them to
  assert the same INDEX.md section formats from node-grammar fixtures.
- `l3 new` keeps printing only the absolute path on stdout (consumers parse
  it).

## Surface after this phase

- One parser: nothing in `apps/cli` parses L3 markdown bodies except crate
  `l3`.
- `l3 list/check/index` work on new-format docs and degrade gracefully on
  old-format ones; INDEX.md keeps its three-section format, now
  graph-derived.
- `l3 new` scaffolds emit the node grammar (anchor-less headings).
- `l3-conventions.md` documents the node grammar as the settled contract;
  SKILL.md points at it.
