# `--from <role>` on the derive verbs, so a work can hold more than one root artifact

## Motivation

A catalog entry can hold several unrelated source artifacts — the Jung entry holds `fulltext`
(a reflowed ebook) and `fulltext-1964-scan` (a re-OCR'd scan of the printed edition), with
different pagination and different trustworthiness. Today every derive verb reads the literal
string `"fulltext"`:

```rust
apps/cli/src/main.rs:253   store.get_content(&args.id, "fulltext")?   // extract-text
apps/cli/src/main.rs:497   store.get_content(id, "fulltext")?         // chunk
```

So `extract-text` can choose its *output* role (`--role`, default `text`) but never its input.
Deriving text from the scan was only possible by extracting it by hand outside braincrawl and
pushing the result with `library put --role text-1964-scan`. The lineage exists only as a
role-slug naming convention that nothing reads or enforces.

This phase makes the input selectable. It does not add lineage to the data model — that is
phase 3 of this chain.

## Do NOT

- Do **not** add a `derived_from` field, a migration, or any schema change. This phase is
  CLI-only. Phase 3 owns the data model.
- Do **not** change the default. `--from` defaults to `fulltext` so every existing invocation
  and every doc example keeps working unchanged.
- Do **not** rename `--role`. It stays the *output* role on both verbs. Adding `--from` as the
  input is what removes the asymmetry; renaming would break callers for no gain.
- Do **not** infer an output role from the input role (e.g. `fulltext-1964-scan` → `text-1964-scan`).
  Silent name derivation is a guess about intent. The caller passes `--role` when they want a
  non-default output.
- Do **not** touch `library fetch`, which acquires the root artifact rather than deriving from
  one, nor `library put`/`get`/`list`.

## Plan

### 1. Add `--from` to both arg structs

In `apps/cli/src/cli.rs`, add to `ExtractTextArgs` (line 257) and `ChunkArgs` (line 107):

```rust
/// Source artifact role to derive from
#[arg(long, default_value = "fulltext")]
pub from: String,
```

Place it directly above the existing `--role` field in each struct so the input/output pair
reads together in `--help`.

### 2. Replace the hardcoded reads

In `apps/cli/src/main.rs`, the `LibraryCmd::ExtractText` arm (line 251) and the chunk path
(around line 497, reached from `run_chunk`): replace `"fulltext"` with the `--from` value.

Both sites also carry `"fulltext"` inside their error messages (lines 257, 292, 299, 501, 510,
516). Those must name the role that was actually requested, not the literal word `fulltext` —
an error saying "no fulltext artifact in store" when the user asked for `fulltext-1964-scan`
sends them looking in the wrong place. Thread the role through.

### 3. Keep the not-a-PDF check honest

Both verbs reject a non-PDF input. That check stays, but its message must name the role read
(`apps/cli/src/main.rs:256-260` and `:500-504`).

## Files to Modify

- `apps/cli/src/cli.rs` — `from` field on `ExtractTextArgs` and `ChunkArgs`
- `apps/cli/src/main.rs` — both dispatch sites, plus every error message that names `fulltext`
- `apps/cli/src/chunk.rs` — only if `run_chunk`'s signature needs the role threaded through
- `apps/cli/tests/` — a test that `--from` reaches the read, and that an absent source role
  produces an error naming that role rather than `fulltext`

## Verification

```bash
cargo test
cargo build --bin braincrawl
```

## Out of Scope

- Recording lineage in the artifact record — phase 3.
- `paginate`, `outline`, `read` — phase 2.
- Any change to how `chunk` partitions text.

## Notes

- The two-editions case that motivates this is real and stored: see the `#reference` node of
  `jung-man-and-his-symbols.l3.md` in the L3 collection, which documents both copies and their
  separate role families.

## Surface after this phase

- `ExtractTextArgs` and `ChunkArgs` each carry a public `from: String` field, defaulting to
  `"fulltext"`, exposed as `--from`.
- `library extract-text <id> --from <role>` and `library chunk <id> --from <role>` read their
  source artifact from `<role>` instead of the hardcoded `"fulltext"`.
- Every error message on those two paths names the requested source role.
- `--role` still means the **output** role on both verbs, unchanged and still defaulting to
  `text` and `chunks` respectively.
- No change to `Artifact`, to any store trait, to the HTTP surface, or to the schema. The
  artifact record still carries no parent pointer, and role slugs remain the only trace of
  lineage.
- `library fetch`, `put`, `get`, and `list` are untouched.
