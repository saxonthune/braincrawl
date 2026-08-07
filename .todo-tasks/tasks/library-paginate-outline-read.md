# `library paginate`, `library outline`, and `library read` — address a book the way a reader does

## Motivation

A reading-guide session is almost entirely "the user names a place in the book, I read that
place and quote it". The addresses they actually use, taken from the DeLanda and Jung sessions
in the L3 conversation store:

> "continuing delanda new philosophy reading guide. **on p 14.** what does analytic and synthetic mean"
> "**I have moved onto chapter 3.**"
> "he hits on the temporal aspect a little **towards the end of ch 2**"
> "give a prereading guide for **ch 3** from von franz" · "detailed overview of **ch 4**"
> "ideally … we also have **a split up version of the pdf where we can get page numbers easily**"

None of that is reachable today. `library get --role text` returns one blob with no page
boundaries, so the DeLanda session ran entirely on line numbers:

```bash
library get --role text > delanda.txt
grep -n -i "intensiv|intensity|extensiv" delanda.txt
sed -n '2015,2032p;2190,2230p;2400,2412p' delanda.txt
```

`grep` gives a line; nothing maps a line back to a page. Across the two sessions this produced
thirteen throwaway Python scripts — `grep_pages`, `folios`, `map_pages`, `find_phrase`,
`place_pages`, `make_paged_text`, `ch4_pages`, `ch5_pages` and more — and one wrong citation
(p.11 for a passage on p.12), which is the failure that matters: a finding whose page is wrong
is worse than no finding.

Half the machinery exists. `pdf_text::extract_pages` (`apps/cli/src/pdf_text.rs:13`) already
returns one normalized `String` per PDF page, exactly and correctly. Artifact roles are free
slugs (`ArtifactRole::parse`, `crates/core/src/types.rs:43`), so new roles need no schema
change. What is missing is a stored page artifact, a stored table of contents, and one verb
that resolves a human address into pages.

## Do NOT

- Do **not** store a single global printed-to-PDF offset. Record a folio **per page** so the
  offset is derived and nothing does arithmetic. A global offset breaks on roman-numeral front
  matter, inserted plates, and multi-part books.
- Do **not** guess a folio. An undetected or inconsistent folio is recorded as `null` and
  reported. A confidently wrong page number is the exact failure being fixed.
- Do **not** detect chapter boundaries automatically, or read PDF bookmarks. The outline is
  entered by hand from the table of contents. That is what the reading guides already do.
- Do **not** shell out to `pdftotext`, `pdfinfo`, or any poppler binary. `extract_pages` does
  this in-process already.
- Do **not** add OCR, or try to read pages with no text layer.
- Do **not** change how `chunk` partitions text, or change its existing `page_start`/`page_end`
  fields.
- Do **not** put resolution logic in the dispatch arm. `resolve` is a pure function over
  already-loaded artifacts, with no store access — that separation is the point of the design.
- Do **not** add a `derived_from` field or any schema change. Phase 3 owns that.

## Plan

### 1. `apps/cli/src/pages.rs` — the page artifact

Follow the shape of `apps/cli/src/chunk.rs`: pure functions over `&[String]`, serializable
record types, unit tests in-file.

```rust
pub struct PageRecord { pub pdf_page: usize, pub folio: Option<String>, pub text: String }
pub struct Pages { pub source_role: String, pub page_count: usize,
                   pub folio_method: FolioMethod, pub pages: Vec<PageRecord> }
pub enum FolioMethod { Detected, Anchored, None }
```

`folio` is a `String` so roman-numeral front matter (`xii`) is representable. `source_role`
records which artifact this was derived from — this is what lets two editions of one work
coexist, and it is the field phase 3 will promote into the artifact record.

`detect_folios(pages: &[String]) -> Vec<PageRecord>`:

- Consider candidate tokens only on the **first and last non-blank lines** of each page. A
  folio sits in a header or footer; restricting the search is what stops body-text numerals
  being mistaken for one.
- A candidate is a token of all arabic digits, or all lowercase roman-numeral characters.
- Resolve by **sequence agreement**, not per-page confidence: the correct series increments by
  one across consecutive PDF pages. For each page choose the candidate consistent with its
  neighbours; where none is consistent, `folio: None`.

`validate_folios(records: &[PageRecord]) -> Vec<FolioBreak>` reports each place the sequence
fails to increment and each page with no folio. Distinguish two cases, because conflating them
makes the report useless on every real book:

- a **gap** — one or more `None` pages between two consistent folios (an unnumbered plate, a
  part title, front matter). Informational.
- a **contradiction** — a detected folio inconsistent with its neighbours. A warning.

This is the only fuzzy step in the feature; splitting pages is exact. It must report its own
failures rather than present a guess as fact.

### 2. `apps/cli/src/outline.rs` — the table of contents

```rust
pub struct Section { pub id: String, pub title: String, pub level: usize,
                     pub start_pdf_page: usize, pub end_pdf_page: usize }
pub struct Outline { pub source_role: String, pub sections: Vec<Section> }
```

This is the `#reference` node's prose table promoted to data. A real one, from
`delanda-new-philosophy-society.l3.md`:

> ch.1 Assemblages against Totalities 9-26 (16-33) → `fulltext-ch01`

`id` is the caller's handle (`ch01`, `intro`, `part3`) and is what `library read --section`
takes. Validate on load: ids unique, ranges non-overlapping, ranges within `page_count`.

### 3. `apps/cli/src/locator.rs` — the seam

Pure. No store access, no IO, fully unit-testable.

```rust
pub enum Locator {
    Printed(PageRange),
    Pdf(PageRange),
    Section { id: String, head: Option<usize>, tail: Option<usize> },
    Quote(String),
}
pub struct PageSpan { pub pdf_pages: Vec<usize> }

pub fn resolve(loc: &Locator, pages: &Pages, outline: Option<&Outline>)
    -> Result<PageSpan, LocatorError>;
```

Every address form collapses to `PageSpan`; nothing downstream knows which form produced it.
Adding an address form later is a variant plus a match arm, and the reader never changes.

`LocatorError` is data, not strings — the CLI formats it, the resolver never prints:

```rust
pub enum LocatorError {
    FolioNotFound { folio: String, pages_lacking_folio: usize },
    PdfPageOutOfRange { requested: usize, page_count: usize },
    NoOutline,
    SectionNotFound { id: String, known: Vec<String> },
    QuoteNotFound { needle: String },
    PartialRange { found: Vec<usize>, missing: Vec<String> },
}
```

`FolioNotFound` carries the count of folio-less pages because that is almost always the real
cause. `SectionNotFound` carries the known ids so the message can list them.

Resolution rules: `Printed` and `Pdf` match **exactly**, never by arithmetic. `Section`
resolves through the outline to a PDF range, then `head`/`tail` take the first or last N pages
of it. `Quote` searches page text and returns the pages containing it. A partly-present range
returns `PartialRange` with what was found — the caller prints what exists and names what is
missing rather than failing the whole request.

### 4. `library read` — the read verb

```
braincrawl library read <id> --printed 9-12
braincrawl library read <id> --section ch01 --first 5
braincrawl library read <id> --find "territorialization"
```

Args: positional `id`; `--printed`, `--pdf`, `--section`, `--find` mutually exclusive with
exactly one required (clap's `required_unless_present_any` and `conflicts_with_all`);
`--first N` / `--last N` valid only with `--section`; `--pages-role` (default `pages`) and
`--outline-role` (default `outline`) to name the artifacts.

Print each page's text to stdout preceded by a marker line:

```
=== p.9 (pdf 16) ===
```

The marker goes to **stdout, not stderr** — it is part of the content, and it is what lets a
model quote and cite in one pass without a second lookup. Where a page has no folio, print
`=== pdf 16 (no folio) ===`.

### 5. `library paginate` and `library outline` — the write verbs

```
braincrawl library paginate <id> [--from fulltext] [--role pages] [--force] [--stdout]
braincrawl library outline  <id> --from-file <toc.json> [--role outline] [--force]
```

`paginate` reads the source artifact, rejects a non-PDF the same way `extract-text` does,
calls `extract_pages`, runs `detect_folios`, reports `validate_folios` breaks to stderr as a
count plus the offending PDF pages, and stores the JSON. Honour the same already-present check
`extract-text` performs unless `--force`.

`outline` reads a JSON file of sections, validates it, and stores it. Hand-authored input is
the whole point — there is no detection step.

Both take `--from` with the same meaning phase 1 established.

### 6. Dispatch

Add `Paginate`, `Outline`, `Read` to `LibraryCmd` (`apps/cli/src/cli.rs:220`) with their arg
structs, and dispatch arms in `apps/cli/src/main.rs` beside the existing library arms. Register
the three new modules in `apps/cli/src/lib.rs`.

## Files to Modify

- `apps/cli/src/pages.rs` — new: `PageRecord`, `Pages`, `FolioMethod`, `FolioBreak`,
  `detect_folios`, `validate_folios`, unit tests
- `apps/cli/src/outline.rs` — new: `Section`, `Outline`, validation, unit tests
- `apps/cli/src/locator.rs` — new: `Locator`, `PageSpan`, `LocatorError`, `resolve`, unit tests
- `apps/cli/src/lib.rs` — register the three modules
- `apps/cli/src/cli.rs` — `Paginate`, `Outline`, `Read` variants and their args
- `apps/cli/src/main.rs` — three dispatch arms
- `apps/cli/tests/` — integration coverage for `paginate` over the existing fixture

## Verification

```bash
cargo test
cargo build --bin braincrawl
```

## Out of Scope

- OCR; pages with no text layer stay unreadable.
- Recovering folios for an edition that prints none (the reflowed Jung ebook). `FolioMethod`
  has an `Anchored` variant reserved for it, but no anchoring is implemented here.
- Teaching `chunk` to carry printed pages.
- Lineage in the artifact record — phase 3.
- Doc updates — phase 4.

## Notes

- `apps/cli/tests/fixtures/hello.pdf` is a single 594-byte page with no folio. It can verify
  the `paginate` plumbing and nothing about detection. **Folio detection must be unit-tested
  with synthetic `&[String]` pages** — `detect_folios` is pure, so this needs no PDF at all.
  Do not try to build detection coverage on the fixture.
- Manual check once it runs: DeLanda `openalex:W3027234447`, 147 PDF pages, printed + 7. Every
  detected folio should be exactly `pdf_page - 7` from where the arabic sequence begins.

## Surface after this phase

- Modules `pages`, `outline`, `locator` exist in `apps/cli/src/` and are registered in `lib.rs`.
- `pages::Pages` / `pages::PageRecord` / `pages::FolioMethod`, `outline::Outline` /
  `outline::Section`, `locator::Locator` / `locator::PageSpan` / `locator::LocatorError` are
  public.
- `locator::resolve(&Locator, &Pages, Option<&Outline>) -> Result<PageSpan, LocatorError>` is
  pure and takes no store handle.
- `LibraryCmd` carries `Paginate`, `Outline`, and `Read` variants; the CLI exposes
  `library paginate`, `library outline`, and `library read` with the flags above.
- The `pages` and `outline` artifact roles hold JSON with a `source_role` field naming the
  artifact they were derived from.
- `library read` writes page text to stdout with `=== p.N (pdf M) ===` marker lines.
- Unchanged and still relied on: `Artifact` carries no parent pointer, roles remain free slugs,
  `extract-text` and `chunk` behave exactly as phase 1 left them, and `chunk`'s
  `page_start`/`page_end` are still PDF pages.
