# Temporary anchors resolved in one batch, so an author never hand-writes an id

## Motivation

Authoring an L3 Research Document today, an author who wants to cross-link two
sibling nodes created in the same batch of edits has two bad options. Anchors do
not exist until `collection assign-ids` runs, and that command only assigns to
headings that lack one (`crates/l3/src/assign.rs:38`). So linking a brand-new node
means either hand-writing a `^r-…` anchor — risky, because anchors are store-global
and an author cannot see the whole store to guarantee uniqueness — or doing a round
trip: write the nodes, run `assign-ids`, read the generated anchors back, then
re-edit every link line to point at them.

This task adds a **temporary anchor**: a `^t-<slug>` an author writes by hand on a
heading and references freely as `[[^t-<slug>]]` from any edge in any doc of the
same batch. `collection assign-ids` mints exactly one fresh `^r-…` per unique
temporary anchor and rewrites the heading anchor and every reference to it, across
the whole store, in one pass. The write→assign→read-back→relink loop collapses to
write→run-once, and the CLI stays the only thing that mints real ids.

Key status-quo fact the design rests on: **the parser already accepts `^t-…`
today.** `split_anchor` (`crates/l3/src/parse.rs:221`) takes any trailing
whitespace-separated `^token` as the anchor, and `Endpoint::resolve("^t-foo")`
yields `Node(NodeId("t-foo"))`. No grammar change is needed. The consequence that
drives pass ordering: a `^t-…` heading has `id.is_some()`, so the existing assign
pass skips it.

## Do NOT

- **Do NOT add a new subcommand.** No `collection resolve-temp-ids`. The resolve
  step is a first pass inside the existing `collection assign-ids`, so no new node
  in `braincrawl-cli.tsp` and no regeneration of `.luminous/cli-grammar.*.json`.
- **Do NOT change the parse grammar.** `split_anchor`, `Endpoint::resolve`, and the
  link forms stay exactly as they are.
- **Do NOT use `str::replace` for the substitution.** `^t-three` is a prefix of
  `^t-three-textures`; a naive replace corrupts the longer id. Match only at a
  token boundary (see step 2).
- **Do NOT change the `^r-…` format, `new_id`, or the store-global uniqueness
  invariant.**
- **Do NOT reuse or generalize `append_anchors`** (`assign.rs:103`) — it appends;
  this pass substitutes in place. Write a separate function.
- **Do NOT touch `crates/l3/src/normalize.rs` or the worker.** Resolution is a
  local authoring step; the worker PUT path is unchanged.
- **Do NOT do any semantic or automatic matching.** The author writes each
  temporary anchor explicitly; the tool only substitutes, it never guesses a link.
- **Do NOT change `assign_ids`' return type or error type.** It stays
  `Result<Vec<Assigned>, std::io::Error>`; the duplicate-definition abort uses
  `std::io::Error::new(std::io::ErrorKind::InvalidData, …)`.
- **Do NOT break the existing behavior** for docs that use no temporary anchor:
  all six existing tests in `crates/l3/src/assign.rs` must still pass unchanged.

## Plan

### 1. Add `temp` to `Assigned`

In `crates/l3/src/assign.rs:12`, add a field to `Assigned`:

```rust
/// The temporary anchor slug (without `^`) this id resolved, when the id came
/// from a `^t-…` rather than from an anchor-less heading.
pub temp: Option<String>,
```

Set `temp: None` at the existing construction site (`assign.rs:46`). Fix the two
existing tests that construct or read `Assigned` if they break.

### 2. Add `resolve_temp_ids_source` to `crates/l3/src/assign.rs`

A private helper beside `append_anchors`:

```rust
/// Replace every `^t-<slug>` token in `content` with `^<real>` per `mapping`,
/// matching only where the token ends at a boundary. Every other byte is
/// preserved. Both the heading anchor form (`## Title ^t-slug`) and the
/// reference form (`[[^t-slug]]`) are the same token, so one scan handles both.
fn resolve_temp_ids_source(content: &str, mapping: &BTreeMap<String, String>) -> String
```

`mapping` is temp slug (without the `^`, e.g. `t-three-textures`) → minted real id
(e.g. `r-k4m9x2p`). Implementation: scan for `^`, take the following run of
`[A-Za-z0-9-]` characters as the token, and substitute only when that whole token
is a key in `mapping`. Taking the maximal character run is what makes the match
boundary-correct: `^t-three-textures` yields the token `t-three-textures`, never
the prefix `t-three`.

### 3. Add the resolve pass to `assign_ids`

Restructure `assign_ids` (`assign.rs:63`) to run two passes over the docs it has
already read, writing each doc once:

1. Parse all docs with `parse_sources` (the `sources` vector already built at
   `assign.rs:74`) and collect:
   - **defined** temporary anchors: every node whose `id` starts with `t-`, keeping
     its doc, heading line, and title;
   - **referenced** temporary anchors: every link whose `source` or `target` is
     `Endpoint::Node(id)` with `id` starting with `t-`.
2. **Duplicate definition is an error.** If the same `t-<slug>` is defined on two
   headings, return `Err(std::io::Error::new(ErrorKind::InvalidData, …))` naming the
   slug and both docs, and write nothing — two headings claiming one identity would
   break store-global uniqueness, which is the invariant this feature exists to
   protect.
3. Build `existing = collect_anchors(&sources)` as today, then mint one
   `new_id(&existing)` per unique defined temporary anchor, inserting each into
   `existing` as it goes, so the anchor-less-heading pass below cannot collide with
   a minted id.
4. **Dangling reference warns, never mints.** A `t-<slug>` referenced by a link but
   never defined on a heading is left in the text untouched, and nothing is minted
   for it. `assign_ids` keeps its exact current signature, so it does not carry the
   warning out; expose the check as a separate public function the CLI calls:

   ```rust
   /// Temporary anchors referenced by a link but never defined on a heading.
   /// Reported by `assign-ids` so a typo surfaces instead of silently persisting.
   pub fn dangling_temp_refs(sources: &[(String, String)]) -> Vec<String>
   ```

   Export it from `crates/l3/src/lib.rs:11` alongside `assign_ids`.
5. Apply `resolve_temp_ids_source` to each doc's content **before**
   `assign_ids_source`, because a `^t-…` heading has `id.is_some()` and the existing
   pass would otherwise skip it forever. Feed the resolved content into
   `assign_ids_source` so headings with no anchor at all still get one.
6. Push one `Assigned { temp: Some(slug), id: minted, … }` per resolved temporary
   anchor, alongside the `Assigned` entries `assign_ids_source` returns. Under
   `dry_run`, report both kinds and write nothing.

### 4. Report resolutions in the CLI

In `cmd_assign_ids` (`apps/cli/src/l3.rs:254`):

- Print a resolution as `{doc}\t{heading_line}\t^{temp} → {id}\t{title}` and an
  ordinary assignment in the current `{doc}\t{heading_line}\t{id}\t{title}` form.
- Before the summary line, call `l3::dangling_temp_refs` over the store's docs and
  print each as `warn: dangling temporary anchor ^{slug} — referenced but never
  defined on a heading`. **Exit 0** — this is advisory, matching
  `collection check`'s posture (`cli.rs:160`).
- Extend the summary line to name resolutions as well as assignments, e.g.
  `2 anchor(s) assigned, 3 temporary anchor(s) resolved`.

### 5. Warn on a surviving temporary anchor in `collection check`

In `lint` (`apps/cli/src/l3.rs:909`), add a warning for any `^t-…` anchor still
present in the body. Without this, `push` sends the temporary anchor to the worker
and `normalize_doc` (`crates/l3/src/normalize.rs:47`) writes it as a real, permanent
anchor in the consolidated store. Message: ``unresolved temporary anchor `^t-…` —
run `collection assign-ids` before pushing``.

`lint` takes `content`, not the parsed graph, so detect it with the same token scan
as step 2, restricted to `## ` heading lines.

### 6. Tests

Add to `crates/l3/src/assign.rs` tests:

- **Resolution across two docs**: doc `a` defines `## X ^t-alpha`, doc `b` has
  `- builds-on [[^t-alpha]]`. After `assign_ids`, both hold the same fresh `^r-…`
  and no `^t-` remains.
- **One id per unique temporary anchor**: two references to `^t-alpha` in different
  docs resolve to one id, not two.
- **Boundary correctness**: a doc defining both `^t-three` and `^t-three-textures`
  resolves each to its own distinct id, and neither id is corrupted.
- **Duplicate definition errors and writes nothing**: assert `assign_ids` returns
  `Err` and the file bytes are unchanged.
- **Dangling reference**: `dangling_temp_refs` names the slug; `assign_ids` leaves
  the reference text unchanged and mints nothing for it.
- **Byte preservation**: as in `write_back_touches_only_the_heading_lines`
  (`assign.rs:158`), assert lines with no temporary anchor are untouched.
- **No regression**: existing docs with no temporary anchor behave exactly as
  before (the existing tests cover this — keep them passing unchanged).

Add to `apps/cli/src/l3.rs` tests: `lint` flags a surviving `^t-…` heading anchor,
and does not flag an ordinary `^r-…` one.

### 7. Update the interface model and the docs

- `braincrawl-cli.tsp:372` — add `temp?: string;` to `AssignedAnchor`.
- `braincrawl-cli.tsp:441` — extend the `assignIds` doc comment: it also resolves
  `^t-…` temporary anchors to fresh `^r-…` ids across the store.
- `.rhidoc/02-architecture/01-core/04-l3-conventions.md:16` and the anchors bullet
  at line 25 — document the temporary anchor: an author may write `^t-<slug>` on a
  heading and reference it as `[[^t-<slug>]]` within a batch; `assign-ids` resolves
  every one to a fresh store-global `^r-…`. A `^r-…` is still never hand-written.
- `.claude/skills/braincrawl/SKILL.md` — the anchors bullet near line 300 and the
  command list near line 319. Show the two-line worked example: the heading
  `## The three textures ^t-three-textures` and the edge
  `- builds-on [[^t-three-textures]]`, then one `collection assign-ids` run.

Do not regenerate `.luminous/` — no clap surface changed.

## Files to Modify

- `crates/l3/src/assign.rs` — `Assigned.temp`, `resolve_temp_ids_source`,
  `dangling_temp_refs`, the resolve pass in `assign_ids`, and new tests
- `apps/cli/src/l3.rs` — `cmd_assign_ids` reporting, `lint` warning, two lint tests
- `braincrawl-cli.tsp` — `AssignedAnchor.temp`, `assignIds` doc comment
- `.rhidoc/02-architecture/01-core/04-l3-conventions.md` — temporary anchor rule
- `.claude/skills/braincrawl/SKILL.md` — anchors bullet and command list

## Verification

```bash
cargo test -p l3
cargo test
cargo build
```

## Out of Scope

- Any change to `crates/l3/src/normalize.rs`, the worker, or the push/pull path.
- Any semantic or automatic link matching.
- Changing the `^r-…` format, `new_id`, or store-global uniqueness.
- A `--dry-run`-only resolve mode separate from the existing `--dry-run` flag.
- Regenerating `.luminous/` canvases.

## Notes

- The `t-` prefix cannot collide with a minted id: `new_id` (`crates/l3/src/ids.rs:15`)
  only ever emits `r-` + 7 chars from a fixed alphabet.
- Ordering is the subtle part. Resolve before assign, and insert every minted id into
  `existing` before the assign pass runs, or a fresh `^r-…` could duplicate a minted one.
- `assign_ids` currently collects all writes and applies them after the loop
  (`assign.rs:94`). Keep that shape: on the duplicate-definition error, no write has
  happened yet, so returning early leaves the store untouched.
- The vocabulary term is **temporary anchor**. Use it in every doc, comment, and
  message. Do not write "temp id", "temp anchor", or "semantic anchor".
