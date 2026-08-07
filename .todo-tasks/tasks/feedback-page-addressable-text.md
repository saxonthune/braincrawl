# Page-keyed `pages` artifact, and text lookup by printed or PDF page

## Motivation

A reading-guide session is almost entirely "the user names a printed page, I read that page and
quote it." braincrawl cannot do that. `library get --role text` returns one 374k-char blob with
no page structure, so a whole session was spent outside the CLI on the original PDF, converting
page numbers by hand — and a citation came out wrong, p.11 reported for a passage on p.12.
Every finding recorded is only worth anything if its page is right.

The hand method was: dump the book with `pdftotext -layout`, `grep -n` for a term (which gives
line numbers, not pages), then separately re-run `pdftotext -f M -l M` on a *guessed* PDF page
and read the bare folio integer off the output, adjusting and retrying until it matched. The
printed-to-PDF offset (+7 in that book) was established by hand and re-verified whenever a
citation mattered, because a single global offset is an assumption with nothing to test it
against.

Triage found half of this already built. `pdf_text::extract_pages`
(`apps/cli/src/pdf_text.rs:13`) already returns one normalized `String` per PDF page, so the
exact part — splitting pages — is done and correct. `chunk_pages` (`apps/cli/src/chunk.rs:31`)
already stamps `page_start`/`page_end` on every chunk, in PDF pages. Artifact roles are free
slugs (`ArtifactRole::parse`, `crates/core/src/types.rs:43`), so a new `pages` role needs no
schema change anywhere. What is missing is folio detection with a self-check, a stored
page-keyed artifact, and a lookup verb.

## Do NOT

- Do **not** store a single global printed-to-PDF offset against the work. That is the
  assumption this task exists to remove — it breaks on roman-numeral front matter, inserted
  plates, and multi-part books. Record a folio **per page**; the offset becomes derived, and
  nothing ever does arithmetic.
- Do **not** guess silently when folio detection is uncertain. An undetected or inconsistent
  folio must be recorded as absent and reported, never invented. A confidently wrong page
  number is the exact failure being fixed.
- Do **not** add OCR, or attempt to read pages with no text layer.
- Do **not** shell out to `pdftotext`, `pdfinfo`, or any poppler binary. `pdf-extract` is
  already a dependency and `extract_pages` already does this work in-process.
- Do **not** change how `chunk` partitions text, or change `chunk`'s existing
  `page_start`/`page_end` fields to printed pages. That knock-on is deliberately deferred.
- Do **not** reflow or clean the extracted text beyond the normalization `extract_text`
  already applies.
- Do **not** invent an id-first command form (`library <id> page …`). Every existing library
  verb is verb-first — `library get <id>`, `library list <id>`, `library chunk <id>` — and
  this task keeps that pattern.

## Plan

### 1. A `PageRecord` type and folio detection

Add a new module `apps/cli/src/pages.rs`, following the shape of `apps/cli/src/chunk.rs`
(a pure function over `&[String]` plus a serializable record type, with unit tests in-file).

```rust
pub struct PageRecord {
    pub pdf_page: usize,        // 1-indexed position, always present
    pub folio: Option<String>,  // detected printed page number; None when undetected
    pub text: String,
}
```

`folio` is a `String`, not a number, so roman-numeral front matter (`xii`) is representable.

Write `detect_folios(pages: &[String]) -> Vec<PageRecord>`:

- For each page, consider only candidate tokens on the **first and last non-blank lines** — a
  folio sits in the header or footer, and restricting the search there is what keeps body-text
  numerals from being mistaken for it.
- A candidate is a token that is entirely arabic digits, or entirely roman-numeral characters
  in lowercase. Ignore tokens attached to other words.
- Resolve candidates by **sequence agreement**, not by per-page confidence: the correct folio
  series increments by one across consecutive PDF pages. Choose, for each page, the candidate
  consistent with its neighbours; where no candidate is consistent, set `folio: None`.

### 2. Self-validation of the folio sequence

Write `validate_folios(records: &[PageRecord]) -> Vec<FolioBreak>`, reporting each place the
detected sequence does not increment by one, plus each page with no folio at all. A
`FolioBreak` names the PDF page and what was seen versus expected.

This is the only fuzzy step in the whole feature — splitting pages is exact — so it must
report its own failures rather than present a guess as fact. Emit the breaks to stderr after
derivation, as a count plus the offending PDF page numbers.

A run of `None` folios across front matter is normal and expected, not an error; report it as
"no folio detected" rather than as a sequence break.

### 3. `library extract-pages` — derive and store the artifact

Add `ExtractPages(ExtractPagesArgs)` to `LibraryCmd` in `apps/cli/src/cli.rs:220`, with args
mirroring `ExtractTextArgs` (`cli.rs:257`): positional `id`, `--role` defaulting to `pages`,
`--force`, `--stdout`.

Dispatch it in `apps/cli/src/main.rs` beside the `LibraryCmd::ExtractText` arm (line 251),
reusing that arm's structure: read the `fulltext` artifact, reject a non-PDF the same way,
call `pdf_text::extract_pages`, run `detect_folios`, report `validate_folios` breaks to
stderr, then store the records as JSON at `--role` (honouring the same already-present check
`extract-text` performs unless `--force`).

Store the artifact as a JSON object with a `pages` array of `PageRecord`, not a bare array, so
the shape has room for per-work fields later without a migration.

### 4. `library page` — look up text by either numbering

Add `Page(PageArgs)` to `LibraryCmd`:

```rust
/// Work id in ns:value form
pub id: String,
/// Printed page number as it appears on the page (folio); accepts a range like 11-13
#[arg(long)]
pub printed: Option<String>,
/// PDF page number (1-indexed position in the file); accepts a range like 18-20
#[arg(long)]
pub pdf: Option<String>,
/// Artifact role holding the page records
#[arg(long, default_value = "pages")]
pub role: String,
```

Exactly one of `--printed` / `--pdf` is required; clap's `required_unless_present` and
`conflicts_with` express that without hand-rolled checks.

Read the `pages` artifact, resolve the request by **exact match** on `folio` or on `pdf_page`
— no arithmetic anywhere in this path — and print the matched pages' text. Print each page's
identity to stderr (`pdf page 19 · printed 12`) so a quoting session can cite without a second
lookup, keeping stdout pure text.

Error cases must be distinguishable and specific:

- No `pages` artifact stored → say so, and name `library extract-pages` as the remedy.
- The requested printed page has no page carrying that folio → say the folio was not found,
  and report how many pages have `folio: None`, since undetected folios are the likely cause.
- A requested range is partly present → print what exists and name the missing numbers on
  stderr; do not fail the whole request.

## Files to Modify

- `apps/cli/src/pages.rs` — new: `PageRecord`, `FolioBreak`, `detect_folios`,
  `validate_folios`, with unit tests
- `apps/cli/src/lib.rs` — register the `pages` module
- `apps/cli/src/cli.rs` — `ExtractPages` and `Page` variants on `LibraryCmd`, plus their args
- `apps/cli/src/main.rs` — dispatch arms for both
- `apps/cli/tests/` — an integration test over the existing fixture
  (`apps/cli/tests/fixtures/hello.pdf`, used by `pdf_text.rs:78`) covering derivation, and
  unit coverage for lookup resolution
- `.claude/skills/braincrawl/SKILL.md` — §4 (progressive disclosure) gains the page-addressable
  read, so a reading-guide session knows to derive once then look up

## Verification

```bash
cargo test
cargo build --bin braincrawl
```

## Out of Scope

- OCR. Pages with no text layer stay unreadable.
- What a "page" means for non-paginated sources (HTML, arXiv abstracts).
- Teaching `chunk` to carry printed pages in `page_start`/`page_end`. Once `pages` exists this
  becomes a small, obvious follow-up that would make quotes come back already cited — file it
  separately rather than widening this task.
- The PDF-to-PDF operations (inspection, cutting a page range out as a new file) — see
  `feedback-pdf-utilities-subcommand.md`.

## Notes

- The verb names `library extract-pages` and `library page` are provisional — proposed during
  triage, not committed to the glossary. `extract-pages` was chosen to sit directly beside the
  existing `extract-text`. `library page` is the form the original report explicitly disliked
  for reading wrong when the id is the subject; it is used here only because every other
  library verb is verb-first and a single id-first exception would be worse. Moving the whole
  namespace to id-first is a much larger change and is not part of this task.
- The report's own hunch suggested also recording the running head per page, which both helps
  detect the folio and tells you which chapter a page is in. Not specified above because
  sequence agreement is the stronger signal and standing alone is simpler to validate; add it
  only if folio detection proves weak on a real book.
- Discovered reading DeLanda, *A New Philosophy of Society* (`openalex:W3027234447`) — 147 PDF
  pages, printed page + 7 = PDF page, 141 pages with a text layer. A good manual check once
  the derivation runs: every detected folio should be exactly `pdf_page - 7` from the point
  the arabic sequence begins.
