# Drop the `updated:` frontmatter field from the L3 envelope

## Motivation

`updated:` is a required L3 frontmatter field that no code reads for any decision.
It is displayed in two places and consumed by nothing:

- `l3 list` prints it as the third column (text and JSON).
- The `INDEX.md` Documents table prints it as the third column.
- `lint` warns when the key is absent (via `REQUIRED_FRONTMATTER`).
- A conformance check asserts a PUT response contains the line.

Nothing sorts, filters, or branches on it. The list sorts by `modified`
(`apps/cli/src/l3.rs:106`). Sync ignores it entirely — `SyncEntry` holds a content
hash and `classify()` is pure over local/remote/last-synced hashes
(`apps/cli/src/l3.rs:348-376`).

Worse, it is inaccurate. `normalize_doc` stamps it only on a write through the
server, but the documented authoring mode is direct disk editing, which never
reaches that path. In the live store, 12 of 26 documents show `modified`
2026-07-21 against `updated` 2026-07-15. The code already documents the problem
at the point of definition: `l3.rs:40` calls it "declared date … (set at
create/import; can drift)" and `l3.rs:776` describes mtime as reflecting "the
real latest edit, unlike the declared `updated:` field."

`modified` (filesystem mtime, `l3.rs:777`) already gives accurate freshness,
derived, with no parsing at all. The field is a slot that invites hand-maintenance
and returns nothing.

## Do NOT

- **Do NOT** add a lint warning for a leftover `updated:` key, the way `domain` is
  flagged as legacy. Every existing document has the key; a warning would fire on
  all 26 and be pure noise. Unknown frontmatter keys are already preserved
  verbatim — leaving them is the intended outcome.
- **Do NOT** edit any `.l3.md` document in the research store. This task changes
  the tooling only. Existing `updated:` lines stay where they are.
- **Do NOT** replace `updated:` with a derived or restamped equivalent, and do not
  make `l3 index` write to document files. The decision is to remove the field,
  not to relocate its ownership.
- **Do NOT** remove or rename `modified`. It stays exactly as it is, including its
  position and its role as the sort key.
- **Do NOT** touch the `import` path's preservation of other frontmatter keys.
  Only the `updated`-specific lines there change.

## Plan

### 1. Stop requiring the key

`apps/cli/src/l3.rs`:

- Line 30: change `REQUIRED_FRONTMATTER` to `&["doc"]`.
- Lines 10-12 (module doc comment): the frontmatter bullet names `doc`, `updated`
  as the keys the tooling reads without parsing the body. Change it to name `doc`
  alone.

### 2. Stop writing the key

`crates/l3/src/normalize.rs`:

- Line 61: delete the `upsert_frontmatter_key(&assigned_content, "updated", today)`
  call; `NormalizeOutcome::Normalized` now carries `assigned_content` directly.
- Line 27-33: remove the now-unused `today: &str` parameter from `normalize_doc`.
- Update the tests in the same file that assert the stamp (around lines 86, 105-111).
  `happy_path_assigns_ids_and_stamps_updated` should lose its `updated` assertion
  and be renamed to describe what it now covers (id assignment). Keep the rest of
  each test's coverage intact.

`apps/cli/src/l3.rs`:

- Line 937 (`cmd_new` header): drop `updated: {}` from the generated frontmatter so
  a new doc is created with `doc:` only. The `today()` call there becomes unused —
  remove it if nothing else in that function needs it.
- Lines 215-216 (`import`): delete both lines. Import should no longer read or
  re-stamp `updated`. An incoming file that already carries the key keeps it,
  because unknown keys pass through `rename_key`/rebuild verbatim.

### 3. Update the two `normalize_doc` call sites

- `apps/server/src/handlers/l3.rs`: line 233 (`let today = today_utc_date();`) and
  the `&today` argument at line 234. If `today_utc_date` (defined at line 89) has no
  remaining caller in that file, remove the function too.
- `apps/worker/src/l3.rs`: line 118 (`let today = crate::today_utc_date();`) and the
  `&today` argument at line 119. `today_utc_date` is defined at
  `apps/worker/src/lib.rs:124` — remove it only if it has no other caller; check
  before deleting.

### 4. Drop the display column

`apps/cli/src/l3.rs`:

- Line 41 and its doc comment at line 40: remove the `updated` field from
  `DocumentMetadata`.
- Line 757: remove the `updated:` initializer.
- Line 110: the text row becomes `doc \t modified \t path`.
- Lines 121-122: remove `"updated"` from the JSON object.
- Lines 854 and 858: the `INDEX.md` table header and rows become
  `| doc | modified | title |`.
- Line 795: the doc comment describing the Documents table as
  "(doc · modified · updated · title)" — drop `updated`.

### 5. Drop the conformance assertion

`crates/conformance/src/lib.rs`, lines 101-103: remove the block asserting the PUT
response contains an `updated:` line. Leave the ` ^r-` anchor assertion above it.

### 6. Update the specification

`.rhidoc/02-architecture/01-core/04-l3-conventions.md`:

- Line 13-14: the frontmatter bullet currently reads "two required fields
  (`collection check` warns if missing): `doc:`, a kebab-case slug that is the
  filename stem and primary key; and `updated:`, a date the CLI stamps." Rewrite it
  for one required field, `doc:`, keeping the slug/primary-key description.
- Line 3 (frontmatter `summary:`): says "two required frontmatter fields (doc,
  updated)". Change to one required field.

State the change as current truth. Do not add a note about the field having been
removed, a deprecation notice, or any changelog narration — history lives outside
the document body.

### 7. Fixture tests that carry the key

Several tests in `apps/cli/src/l3.rs` use fixtures containing `updated:`
(around lines 1100-1103, 1145) and one asserts `fm_get(&fm, "updated")` round-trips
through import. Adjust only what the change breaks:

- The import round-trip assertion at line 1103 no longer holds as a *stamped*
  guarantee, but the key should still survive as an unknown key — verify which
  behavior the test is asserting and keep the pass-through coverage.
- Leave `updated:` in body fixtures where it is incidental; it is valid unknown
  frontmatter.

## Files to Modify

- `apps/cli/src/l3.rs` — required-key list, module doc, `DocumentMetadata`, `cmd_new`
  header, import stamping, list text/JSON output, `INDEX.md` table, affected tests
- `crates/l3/src/normalize.rs` — drop the stamp, drop the `today` parameter, update tests
- `apps/server/src/handlers/l3.rs` — call site, possibly `today_utc_date`
- `apps/worker/src/l3.rs` — call site
- `apps/worker/src/lib.rs` — `today_utc_date` only if it becomes unused
- `crates/conformance/src/lib.rs` — drop the PUT assertion
- `.rhidoc/02-architecture/01-core/04-l3-conventions.md` — summary and frontmatter bullet

## Verification

```bash
cargo build --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

A clean clippy run is the real gate here: the change removes a function parameter
and several struct fields, so any missed call site or unused helper surfaces as a
warning rather than silently compiling.

## Out of Scope

- Removing existing `updated:` lines from documents in the research store.
- Any other frontmatter key. `doc:` is identity and stays required.
- Changing how `modified` is computed or displayed.
- The `l3 split` command, the `collection rename` verb, or any other CLI gap.

## Notes

- The general principle behind this task: tooling owns deterministic values, and a
  value nothing consumes should not exist as a field an author can edit. Removing
  the slot is preferred over writing a rule telling agents not to touch it.
- `import` at lines 209-213 rebuilds frontmatter by preserving every line verbatim,
  so dropping the explicit `updated` handling does not drop the key from files that
  already have it. Confirm this with the existing round-trip test rather than
  assuming it.
- The worker is a proof of concept and is not deployed; change its call site so the
  workspace compiles, and do not investigate worker behavior beyond that.
