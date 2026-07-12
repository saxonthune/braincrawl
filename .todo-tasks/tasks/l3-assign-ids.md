# l3 assign-ids: mint missing node anchors

## Motivation

Node identity is tooling-owned: the agent writes `##` headings, the tool
writes the `^r-…` anchors. This verb is the one file-writing operation in the
L3 graph design.

## Do NOT

- Do NOT rewrite, reformat, or re-key an existing anchor — ever. An anchor
  written once is permanent.
- Do NOT touch any line other than a `##` heading line that lacks an anchor.
  No reflowing, no whitespace normalization, no envelope edits.
- Do NOT use a timestamp or counter in the id — pure random over the fixed
  alphabet.
- Do NOT add the `nanoid` crate; the generator is a few lines over `rand`.

## Plan

### 1. Id generation in `crates/l3`

New module `crates/l3/src/mint.rs` (internal name; the user-facing verb is
`assign-ids`): `pub fn new_id(existing: &HashSet<String>) -> String`.
Format: `r-` prefix + 7 chars from the alphabet
`23456789abcdefghjkmnpqrstvwxyz` (no 0/1/i/l/o/u). Use the `rand` crate
(add to `crates/l3` deps). Loop: generate, regenerate on collision with
`existing`.

### 2. Write-back

`pub fn assign_ids(root: &Path, dry_run: bool) -> Result<Vec<Assigned>, …>`
in `crates/l3`: run `parse()`, collect nodes with `id: None`, and for each,
append ` ^<new-id>` to the heading line (use `Provenance::heading_line`;
re-read the file as lines, modify only those lines, write back). Collect all
existing anchors across the whole store first — uniqueness is store-wide.
`Assigned { doc, path, heading_line, id, title }`.

### 3. CLI verb

- `apps/cli/src/cli.rs`: add `AssignIds { #[arg(long)] dry_run: bool }` to
  `L3Cmd` (enum at `cli.rs:107`), kebab-cased by clap to `assign-ids`.
- `apps/cli/src/l3.rs`: dispatch arm calling `l3::assign_ids`; print one
  line per assignment (`doc  line  id  title`) to stderr, count summary;
  with `--dry-run` print what would be minted and write nothing.
- `apps/cli/Cargo.toml`: depend on the `l3` crate.

### 4. CLI grammar spec

Add the verb to `braincrawl-cli.tsp` and `braincrawl-cli.smithy` (repo
root), following the existing pattern for `l3` verbs in each file.

## Files to Modify

- `crates/l3/src/mint.rs` — new; `crates/l3/src/lib.rs` — export
- `crates/l3/Cargo.toml` — add `rand`
- `apps/cli/src/cli.rs` — `L3Cmd::AssignIds`
- `apps/cli/src/l3.rs` — dispatch arm
- `apps/cli/Cargo.toml` — dep on `l3`
- `braincrawl-cli.tsp`, `braincrawl-cli.smithy` — grammar entries

## Verification

```bash
cargo build
cargo test -p l3
cargo test
```

## Out of Scope

- Migrating existing docs' content; re-keying anchors
- Any other file mutation

## Notes

- Tests to include in `crates/l3`: id alphabet/length/prefix; collision
  regeneration (seed `existing` with a forced hit); write-back touches only
  the heading line (byte-compare the rest of the file); idempotence (second
  run assigns nothing); dry-run writes nothing.
- Anchors are store-wide unique even though references can be doc-qualified —
  uniqueness is checked against every doc, not per file.

## Surface after this phase

- `braincrawl l3 assign-ids [--dry-run]` exists and is in the CLI spec.
- `l3::assign_ids(root, dry_run)` and `l3::mint::new_id(existing)` exported.
- After a run, every `##` heading in the store carries a stable unique
  `^r-…` anchor; anchors never change once written.
