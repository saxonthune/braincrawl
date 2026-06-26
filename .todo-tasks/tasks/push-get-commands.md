# Generalize `push-pdf`/`get-pdf` → `push`/`get` with `--role` and an open `ArtifactRole`

## Motivation

The glossary (`.rhidoc/01-product/01-glossary.md`) settled the vocabulary: a stored
piece of content is an **artifact**, and its **role** (`abstract`, `fulltext`, or a custom
slug like `map`) is the axis distinguishing what the bytes are to the work — independent of
its **mime** (the byte encoding). The CLI is still pinned to the old, PDF/fulltext-only
shape: `push-pdf` and `get-pdf` hardcode the `fulltext` role and bake "pdf" into their names,
and `ArtifactRole` is a closed enum with no escape hatch for any other role.

This task generalizes both commands to any role and opens `ArtifactRole` so a user can push
(and read back) an arbitrary artifact such as an `image/png` map. The store already supports
this — the `role` column is `TEXT`, and the `/works/{alias}/content/{role}` route is
role-agnostic — so this is mostly CLI plumbing plus one enum variant.

## Do NOT

- Do NOT keep `push-pdf` or `get-pdf` as aliases. Replace them outright (rename the
  `Namespace` variants and command names). Pre-release; no back-compat shims.
- Do NOT touch `fetch-content`, `fetch-pdf`, or `extract-text`. Those reach the open web /
  parse PDFs and are out of scope — only the store-facing `push`/`get` pair changes.
- Do NOT add new columns, migrations, or descriptor fields (no filename, no created_at).
  The `role` column is already `TEXT`; nothing in the schema changes.
- Do NOT change the `/content/` route path string, the `put_content`/`get_content` method
  names, or the on-disk role strings (`abstract`/`fulltext` stay byte-identical).
- Do NOT make `ArtifactRole::Other` accept arbitrary text. It MUST be validated to a safe
  slug (see below) because the role string is interpolated into the blob key
  (`{canonical_id}/{role}/v{version}`) and into the URL path.

## Plan

### 1. Open the `ArtifactRole` enum (`crates/core/src/types.rs:25`)

Add a third variant:

```rust
pub enum ArtifactRole {
    Abstract,
    Fulltext,
    Other(String),   // a custom role slug, validated [a-z0-9_-]+, non-empty
}
```

Add two inherent methods on `ArtifactRole` so serialization/parsing lives in one place:

- `pub fn as_str(&self) -> &str` — `Abstract => "abstract"`, `Fulltext => "fulltext"`,
  `Other(s) => s`. (Borrows `self`; returns `&str`.)
- `pub fn parse(s: &str) -> Option<ArtifactRole>` — `"abstract" => Abstract`,
  `"fulltext" => Fulltext`, otherwise: if `s` is non-empty and every char matches
  `[a-z0-9_-]` return `Some(Other(s.to_string()))`, else `None`. (Reject empty, uppercase,
  slashes, dots, whitespace — anything outside the slug charset.)

### 2. Route existing serialize/parse sites through the new methods

- `crates/core/src/usecases.rs:196-199` — replace the inline `match kind { … }` that builds
  `role_str` with `let role_str = kind.as_str();`. Note `kind` is also moved into the
  descriptor later (`role: kind`) and cloned for `next_version`; `as_str` only borrows, so
  order is fine — bind `role_str` before the move and it is used in the `format!` above the
  move. Confirm it still compiles.
- `apps/server/src/lib.rs:174` — make `parse_artifact_role` delegate to
  `ArtifactRole::parse` (or replace its body with the same logic). Both PUT and GET
  dispatch (lines ~286 and ~332) already call it and map `None → 400/invalid role`, so a
  bad slug keeps returning a 400 — good.

### 3. Fix every now-non-exhaustive `match ArtifactRole`

Adding `Other` breaks any exhaustive match. Build the workspace and fix each one
(`apps/worker/src/lib.rs`, backends, tests). For roles the fetch pipeline doesn't handle
(the worker only knows `abstract`/`fulltext`), `Other(_)` should be a clean no-match /
error path consistent with the surrounding code — not a panic.

### 4. Rename the CLI commands (`apps/cli/src/cli.rs`)

- `Namespace::PushPdf(PushPdfArgs)` → `Namespace::Push(PushArgs)`, command `name = "push"`,
  about: "Store bytes from stdin or a file as an artifact of the given role".
- `Namespace::GetPdf(GetPdfArgs)` → `Namespace::Get(GetArgs)`, command `name = "get"`,
  about: "Read a stored artifact's bytes to stdout".
- `PushArgs`: keep `id`, `file`, `mime`, `source`, `source_url`; ADD
  `#[arg(long, default_value = "fulltext")] pub role: String`.
- `GetArgs`: keep `id`, `output`; ADD `#[arg(long, default_value = "fulltext")] pub role: String`.

### 5. Update the handlers (`apps/cli/src/main.rs`)

- `Namespace::PushPdf` arm → `Namespace::Push`. It currently calls
  `store.put_content(&args.id, "fulltext", bytes, mime, source, source_url)`. Pass
  `&args.role` instead of the `"fulltext"` literal. The server validates the role and
  returns 400 on a bad slug — surface that error as-is.
- `Namespace::GetPdf` arm → `Namespace::Get`. It calls `store.get_content(&args.id,
  "fulltext")`; pass `&args.role`. Keep the same `ContentOutcome` handling
  (Bytes → write to `--output`/stdout; Pending; Absent → error).

`store_client.rs` `put_content`/`get_content` already take the role as `&str`, so they need
no signature change.

### 6. Add a round-trip test for a custom role

In the usecase/store engine tests (`crates/core/tests/engine.rs` or
`crates/backends/store-sqlite/tests/engine.rs`, following the existing put/get pattern),
add a test that `put_content` + `get_content` round-trips an `ArtifactRole::Other("map")`
artifact (store bytes with role `map`, read them back, assert bytes/mime match). Also assert
`ArtifactRole::parse("Map")`, `parse("a/b")`, and `parse("")` all return `None`.

## Files to Modify

- `crates/core/src/types.rs` — add `Other(String)` variant + `as_str`/`parse` methods.
- `crates/core/src/usecases.rs` — use `as_str()` for `role_str`.
- `apps/server/src/lib.rs` — `parse_artifact_role` delegates to `ArtifactRole::parse`.
- `apps/worker/src/lib.rs` — fix non-exhaustive match(es) for `Other`.
- `apps/cli/src/cli.rs` — rename `PushPdf`/`GetPdf` variants+args → `Push`/`Get`, add `--role`.
- `apps/cli/src/main.rs` — rename handler arms, pass `&args.role` through.
- backends / tests — fix any other non-exhaustive matches; add the custom-role round-trip test.

## Verification

```bash
cargo build --workspace
cargo test --workspace
cargo run -q -p braincrawl-cli -- push --help
cargo run -q -p braincrawl-cli -- get --help
# push/get --help must each list --role; push-pdf/get-pdf must be gone:
cargo run -q -p braincrawl-cli -- push-pdf --help 2>&1 | grep -q "unrecognized\|unexpected\|error" && echo "push-pdf removed OK"
```

## Out of Scope

- Original-filename and `created_at`-vs-`fetched_at` archival fields (need a schema column) —
  a separate task.
- `fetch-content` / `fetch-pdf` / `extract-text` naming.
- Any cryptographic-fixity (sha256) change to `content_hash`.

## Notes

- The whole point of the `[a-z0-9_-]+` validation is that `role` becomes a blob-key path
  segment and a URL path segment; an unsanitized slug could escape the key namespace or
  break the route. Keep the validation in `ArtifactRole::parse` so client and server agree.
- The agent runs without a live server, so end-to-end `push`/`get` over HTTP can't be
  exercised — the round-trip test goes through the usecase/store layer instead, which is
  where the role logic lives.
