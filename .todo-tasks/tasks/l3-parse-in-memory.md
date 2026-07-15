# crates/l3: in-memory parse + assign (wasm-usable API)

## Motivation

The Cloudflare worker will store L3 docs as R2 objects and must parse and
normalize them, but `crates/l3` is filesystem-bound: `parse(root: &Path)`
walks a directory, and `assign_ids(root, dry_run)` writes files in place.
This phase extracts in-memory equivalents so the same logic runs against
doc text held in memory (worker) and against files on disk (CLI), with the
fs functions becoming thin wrappers.

## Do NOT

- Do NOT change the parse grammar, the `Graph`/`Node`/`Link` model, their
  JSON serialization, or the id alphabet/length in `ids.rs`.
- Do NOT change the behavior of the existing `parse(root)` and
  `assign_ids(root, dry_run)` signatures — all current callers (server
  handler, CLI) must compile and behave identically.
- Do NOT touch `apps/worker` or `apps/cli` beyond what compiling requires
  (they should need no changes if the fs wrappers keep their signatures).
- Do NOT introduce new dependencies to `crates/l3`.

## Plan

### 1. In-memory parse

In `crates/l3/src/parse.rs`, refactor so the per-document parsing operates
on `(doc_name, content)` pairs:

- Add `pub fn parse_sources(sources: &[(String, String)]) -> (Graph, Vec<Warning>)`
  where each tuple is (doc slug, full markdown text). Semantics identical
  to `parse` on a directory containing exactly those docs: never fails,
  malformed input becomes `Warning`s. `Provenance.path` for in-memory docs
  should be `PathBuf::from(format!("{slug}.l3.md"))`.
- Reimplement `parse(root: &Path)` as: walk the tree exactly as today
  (same skip rules: `_`-prefix files/dirs, `INDEX.md`, non-`.l3.md`), read
  each file, delegate to the same per-doc core as `parse_sources`, but keep
  the real `path` in `Provenance`.
- Export `parse_sources` from `lib.rs`.

### 2. In-memory assign

In `crates/l3/src/assign.rs`:

- Add `pub fn assign_ids_source(content: &str, existing: &mut HashSet<String>) -> (String, Vec<Assigned>)`:
  given one doc's markdown and the set of anchors already taken store-wide,
  append ` ^<id>` to every anchor-less node heading (byte-preserving for
  all other lines, exactly like today's `write_back`), inserting each new
  id into `existing` as it goes. `Assigned.path` may be a synthetic
  `<doc>.l3.md` path; `Assigned.doc`/`heading_line`/`id`/`title` as today.
- Add `pub fn collect_anchors(sources: &[(String, String)]) -> HashSet<String>`
  (parse the sources, gather all existing `NodeId`s).
- Reimplement `assign_ids(root, dry_run)` on top of these: read files,
  collect anchors store-wide, run `assign_ids_source` per doc, write files
  back only when `!dry_run`. Behavior (ordering, dry-run output) unchanged.

### 3. Frontmatter helper

The CLI has a private frontmatter `upsert_key` in `apps/cli/src/l3.rs`.
Add a string-level equivalent to the crate —
`pub fn upsert_frontmatter_key(content: &str, key: &str, value: &str) -> String`
(new small module `crates/l3/src/frontmatter.rs`, exported from lib.rs) —
matching the CLI helper's semantics (replace the key's line if present in
the frontmatter block, else insert it; create a frontmatter block if the
doc has none). Switch `apps/cli/src/l3.rs` to call it so there is one
implementation (this is the one permitted CLI edit).

### 4. Tests

Unit tests in `crates/l3` (follow the existing test style in the crate —
check for `#[cfg(test)]` modules or `tests/`):
- `parse_sources` equivalence: build a temp dir with 2–3 small docs, assert
  `parse(dir)` and `parse_sources(same contents)` produce equal node/link
  sets (ignoring `Provenance.path`).
- `assign_ids_source`: anchor-less headings get ids; existing anchors
  untouched; other bytes preserved; ids unique against the passed set.
- `upsert_frontmatter_key`: replace, insert, and no-frontmatter cases.

## Files to Modify

- `crates/l3/src/parse.rs` — extract per-doc core, add `parse_sources`
- `crates/l3/src/assign.rs` — extract in-memory assign, add `collect_anchors`
- `crates/l3/src/frontmatter.rs` — new helper module
- `crates/l3/src/lib.rs` — export the new API
- `apps/cli/src/l3.rs` — use `upsert_frontmatter_key` from the crate

## Verification

```bash
cargo test -p braincrawl-l3
cargo build --workspace
```

## Out of Scope

- Any worker or HTTP surface (next phase)
- Markdown re-serialization from `Graph` (never needed — normalization is
  line-level: anchor append + frontmatter upsert)

## Notes

- `std::fs` remaining in the fs wrappers is fine for wasm: the worker will
  only call the in-memory functions.
- Keep the per-doc parse core private; only the two entry points plus
  `collect_anchors` and the frontmatter helper join the public API.

## Surface after this phase

- `braincrawl_l3::parse_sources(&[(String, String)]) -> (Graph, Vec<Warning>)`
- `braincrawl_l3::assign::assign_ids_source(&str, &mut HashSet<String>) -> (String, Vec<Assigned>)`
  (re-exported at crate root like `assign_ids`)
- `braincrawl_l3::collect_anchors(&[(String, String)]) -> HashSet<String>`
- `braincrawl_l3::upsert_frontmatter_key(&str, &str, &str) -> String`
- `parse(root)`, `assign_ids(root, dry_run)`, `ids::new_id`, model types
  and their JSON serialization: unchanged signatures and behavior.
- Negative space: no worker/server/CLI route changes; `apps/cli` behavior
  identical; `crates/l3` has no new dependencies.
