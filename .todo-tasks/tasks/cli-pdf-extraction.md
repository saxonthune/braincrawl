# PDF text extraction — first slice: wrap pdf-extract as a CLI verb

## Motivation

Doing Mesopotamia research, the progressive-disclosure loop (metadata graph →
abstracts → AI-condensed summary → full text) stalls when a work is open access
with a clean PDF but there is no way to turn that PDF into text the agent can
read or condense. The acquisition half (resolve OA URL → download bytes → store
as a `fulltext` PDF payload) already exists in `fetch-content`. The missing piece
is **PDF bytes → readable text**.

This is the **first slice only**: add the `pdf-extract` dependency and expose a
thin `extract-text` command that reads an already-stored PDF payload and prints
the extracted text to stdout. Persistence as a new payload, the lossy-table /
missing-image problem, and a vision-based high-fidelity rung are all deferred —
see Out of Scope. The goal here is to get extraction working end-to-end behind a
command we can iterate on.

## Do NOT

- Do NOT write the extracted text back to the store. This slice prints to stdout
  only. (Where extracted text should land — a new `fulltext` version vs. a
  distinct `Plaintext` payload kind — is a deliberately deferred decision; do not
  pre-empt it by clobbering the stored PDF `fulltext` payload.)
- Do NOT add a new `PayloadKind` variant or touch `crates/core`, `crates/sql`, or
  the server. This slice is contained to `apps/cli`.
- Do NOT add OA-URL resolution or downloading to this command — it operates on the
  PDF that is **already in the store**. If none is present, error and tell the user
  to run `fetch-content` first. (Re-acquiring bytes is `fetch-content`'s job.)
- Do NOT attempt OCR, image extraction, table-structure recovery, or page
  rendering. Born-digital text only; these are out of scope.
- Do NOT route the output through the JSON `Envelope` renderer — this command
  emits raw extracted text, like `fetch-content` emits a plain status line.
- Do NOT name the new CLI module `pdf_extract` — it would collide with the
  `pdf_extract` crate. Use `pdf_text`.

## Plan

### 1. Add the dependency — `apps/cli/Cargo.toml`

Add `pdf-extract = "0.7"` (or the current 0.x) to `[dependencies]`. It is pure
Rust with no system-library requirement — confirm it builds in this workspace
before going further.

### 2. Extraction wrapper — `apps/cli/src/pdf_text.rs` (new module)

A thin, server-free, unit-testable wrapper:

```rust
/// Extract UTF-8 text from born-digital PDF bytes.
pub fn extract_text(bytes: &[u8]) -> Result<String, Box<dyn std::error::Error>>
```

- Validate `bytes` begins with `%PDF` (reuse `fetch_content::is_valid_pdf`); if
  not, return a clear error rather than handing garbage to the crate.
- Call `pdf_extract::extract_text_from_mem(bytes)`.
- Apply only **minimal** normalization for now: trim trailing whitespace per line
  and collapse 3+ consecutive blank lines to one. Keep it small — richer
  dehyphenation / layout cleanup is a later slice. Put it in a private
  `fn normalize(raw: String) -> String` so it has a unit-test seam.

Register the module in `apps/cli/src/lib.rs` (`pub mod pdf_text;`).

### 2. CLI surface — `apps/cli/src/cli.rs`

- Add a `Namespace::ExtractText(ExtractTextArgs)` variant with
  `#[command(name = "extract-text", about = "Extract text from a work's stored fulltext PDF payload")]`.
- `ExtractTextArgs { /// Work id in ns:value form (e.g. openalex:W2304167012)\n pub id: String }`.

Follow the `FetchContent` / `FetchContentArgs` pattern already in this file.

### 3. Wire the command — `apps/cli/src/main.rs`

Add a `Namespace::ExtractText(args)` arm that:

1. Builds a `StoreClient` (same as the `FetchContent` arm).
2. Calls `store.get_content(&args.id, "fulltext")`.
3. Matches the `ContentOutcome`:
   - `Bytes { bytes, mime }` → if `mime` does not look like PDF and the bytes are
     not `%PDF`, return an error (`fulltext payload for {id} is not a PDF (mime={mime})`).
     Otherwise call `pdf_text::extract_text(&bytes)`, `println!` the result, and
     print a `eprintln!` summary line (e.g. `extracted: {n} chars`).
   - `Absent` → `Err("no fulltext payload in store for {id}; run fetch-content first")`.
   - `Pending` → `Err("fulltext for {id} is still being fetched")`.
   - `Restricted` → `Err("fulltext for {id} is rights-restricted")`.
   - `Redirect(url)` → `Err("only a link is stored for {id} (link-only): {url}")`.

Keep the arm thin; the extraction logic lives in `pdf_text`.

## Files to Modify

- `apps/cli/Cargo.toml` — add `pdf-extract` dependency.
- `apps/cli/src/pdf_text.rs` — new: `extract_text` + `normalize` + unit tests.
- `apps/cli/src/lib.rs` — `pub mod pdf_text;`.
- `apps/cli/src/cli.rs` — `ExtractText` namespace variant + `ExtractTextArgs`.
- `apps/cli/src/main.rs` — wire the `extract-text` arm.
- `apps/cli/tests/fixtures/hello.pdf` — new: a tiny born-digital PDF containing a
  known string (e.g. "Hello braincrawl") for the extraction test.

## Verification

```bash
cargo build -p braincrawl-cli
cargo test -p braincrawl-cli
cargo clippy -p braincrawl-cli --all-targets -- -D warnings
```

Add tests in `pdf_text.rs`:
- `normalize` collapses 3+ blank lines to one and trims trailing line whitespace.
- `extract_text` on non-PDF bytes (e.g. `b"<html>"`) returns an error.
- `extract_text` on the committed `tests/fixtures/hello.pdf` fixture returns text
  containing the known string. (If embedding the fixture inline as a byte slice is
  simpler than a file, that is acceptable — the point is a server-free assertion
  that the crate is wired correctly.)

## Out of Scope

- Persisting extracted text back to the store (next slice — gated on the deferred
  payload-kind decision).
- Adding a `PayloadKind::Plaintext` or any `crates/core`/`sql`/server change.
- Re-acquiring/downloading bytes or OA-URL resolution (that is `fetch-content`).
- Tables, images, figures: born-digital text extraction mangles tables and drops
  images entirely. A high-fidelity rung (page rasterization → vision model) is a
  separate future task; do not attempt it here.
- OCR of scanned/image-only PDFs.
- Richer text cleanup (dehyphenation across line breaks, column/footnote handling).

## Notes

- `pdf-extract` is born-digital-only and is known to produce positional, lossy
  output for tables and to ignore images — acceptable for this prose-skimming
  slice, but the reason persistence and the read path are deferred (a lossy text
  payload should not bury the high-fidelity PDF).
- Real example to test against once wired: `openalex:W2304167012`, oa_url
  `https://jwsr.pitt.edu/ojs/jwsr/article/download/288/300` (diamond OA, CC-BY,
  born-digital). First `fetch-content` it, then `extract-text` it.
- `get_content` already returns `ContentOutcome::Bytes { bytes, mime }`
  (`apps/cli/src/store_client.rs`); no new client method is needed.

## Surface after this phase

- `braincrawl_cli::pdf_text::extract_text(&[u8]) -> Result<String, Box<dyn Error>>`
  — public, server-free PDF-bytes → text wrapper. Later slices (persistence,
  cleanup) build on this entry point.
- A `braincrawl extract-text <id>` CLI verb that reads the stored `fulltext` PDF
  payload and prints extracted text to stdout. It does **not** write to the store
  — persistence is deliberately still open.
- The stored PDF `fulltext` payload is left untouched and authoritative; nothing
  in this slice creates a competing text payload.
