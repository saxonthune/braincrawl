# `pdfutils` — inspect a PDF and cut a page range out of it

## Motivation

Loading a user-supplied book into the Library took four shell tools braincrawl does not wrap,
and each is a step that repeats on every book. braincrawl already owns the neighbouring
operations (`rename`, `library put`, `extract-text`, `chunk`), so the PDF work sits in an odd
gap: the CLI knows how to store and derive from a PDF but cannot tell you anything about one
or cut one up.

What was reached for outside braincrawl:

- **`pdfinfo`** — page count, born-digital versus scanned, producer, page size, needed to
  answer "is this splittable?" before committing to anything. `chunk` half answers it, but only
  by failing (`6/147 pages have no text layer`), and only after storing and deriving.
- **`pdfseparate` + `pdfunite`** — cut a chapter out to store under its own role. Two commands
  and a scratch directory of 18 intermediate single-page files to get one chapter, because
  poppler has no "extract this page range as one file" verb.

Two things a wrapper should know about. The split is grossly inflated: chapter 1 (18 pages)
came out at 2.98 MB against the whole 147-page book's 3.14 MB, because poppler copies the full
font and resource set into every piece. Splitting all five chapters would cost roughly 5× the
original in the blob store. `qpdf --pages` does this properly but was not installed, and
nothing warned about it. And the output is noisy: dozens of `Syntax Warning` lines and font
glyph-name warnings, with one `pdfseparate` run producing 12.8 MB of warnings.

## Scope reduction — read this first

This task is **smaller than the original report**. Triage moved two of its items elsewhere:

- Its top want, "read a page range as text", is superseded by
  `feedback-page-addressable-text.md`, which does it exactly and by *printed* page against a
  stored per-page artifact rather than by re-extracting from the original file. Do not
  implement a text-reading verb here.
- Its hunch about recording a printed-to-PDF offset against the work is explicitly rejected by
  that same task, which records a folio per page so the offset is derived, never stored.

What remains is inspection, page-range extraction to a new PDF, and warning suppression.

## Do NOT

- Do **not** add a "read a page range as text" verb. That belongs to `library page`.
- Do **not** record a printed-to-PDF page offset anywhere.
- Do **not** add OCR. `chunk --allow-partial` already handles skipping image-only pages.
- Do **not** decide chapter boundaries automatically or read PDF bookmarks. Boundaries are
  read off the table of contents by hand and that is fine.
- Do **not** change how `chunk` partitions text.
- Do **not** fail silently or fall back to a worse backend when the preferred external tool is
  absent. Silence about a missing `qpdf` is the specific failure being fixed — say which tool
  is missing and what installing it would change.

## Plan

### 1. A `pdfutils` namespace

Add `Pdfutils(PdfutilsArgs)` to `Namespace` in `apps/cli/src/cli.rs:45`, with a `PdfutilsCmd`
subcommand enum. Every subcommand takes a source that is either a stored artifact or a loose
file on disk, since these are used *before* deciding to store anything:

- positional `id` in `ns:value` form, resolved against `--role` (default `fulltext`), or
- `--file <path>` for a loose file, mutually exclusive with `id`.

Factor that source resolution into one helper both subcommands call, returning the PDF bytes
and a label for messages. `ChunkArgs` (`cli.rs:107-112`) already models the same either-or
shape and is the local precedent to follow.

### 2. `pdfutils info`

Report, for the resolved PDF: page count, per-page text-layer presence, producer, and page
size.

Page count and text-layer presence come free and in-process from existing code —
`pdf_text::extract_pages` (`apps/cli/src/pdf_text.rs:13`) plus `chunk::unfaithful_pages`
(`apps/cli/src/chunk.rs:19`), which is exactly the `6/147 pages have no text layer` figure the
report wanted without having to store and derive first. Prefer that over shelling out.

Producer and page size are not available through `pdf-extract`. Get them from `pdfinfo` when
present; when it is absent, report those two fields as unavailable and name `pdfinfo` — do not
fail the whole command, since the two figures that actually answer "is this splittable?" came
from in-process code.

Render through the existing `Envelope` / `render` path (`apps/cli/src/output.rs:29`) so
`--json` and `--text` behave as they do everywhere else.

### 3. `pdfutils extract` — a page range as a new PDF

Take `--from N --to M` (1-indexed PDF pages, inclusive) and produce a new PDF. Write it to
`--output <path>`, or store it against the work at `--role <slug>` when the source was a
stored artifact.

Backend selection, in order, with the choice always reported on stderr:

1. **`qpdf --pages`** when on `PATH` — it subsets fonts and resources properly, which is what
   avoids the 2.98 MB chapter.
2. **`pdfseparate` + `pdfunite`** as a fallback, in a temporary directory that is always
   cleaned up.

When falling back, warn plainly and quantitatively: name that `qpdf` is absent, that poppler
copies the full resource set into each piece, and — after writing — report the output size
against the source size when the output exceeds roughly half the original. That is the warning
whose absence cost a 2.98 MB chapter.

When neither backend is present, fail with a message naming both and what each provides.

### 4. Suppress the poppler warning stream

Every external invocation captures stderr rather than letting it through, and discards it
unless the command failed or `--verbose` was passed. Add `--verbose` as a `pdfutils`-level
flag.

`extract-text` and `chunk` leak the same noise today. Apply the same suppression to them if it
is a small, local change at their process boundary; if they do not shell out at all and the
noise originates inside `pdf-extract`, leave them alone and note it — do not restructure
either command chasing this.

## Files to Modify

- `apps/cli/src/pdfutils.rs` — new: source resolution, `info`, `extract`, backend selection
- `apps/cli/src/lib.rs` — register the module
- `apps/cli/src/cli.rs` — `Pdfutils` namespace variant, `PdfutilsCmd`, args
- `apps/cli/src/main.rs` — dispatch arm
- `apps/cli/tests/` — unit coverage for backend selection (present/absent/neither, without
  invoking the tools) and for source-argument validation
- `.claude/skills/braincrawl/SKILL.md` — one line in §4 noting `pdfutils info` answers
  "is this splittable?" before storing anything

## Verification

```bash
cargo test
cargo build --bin braincrawl
```

## Out of Scope

- Reading a page range as text — `library page`, from `feedback-page-addressable-text.md`.
- Any printed-to-PDF offset.
- OCR, automatic chapter detection, bookmark-driven splitting.
- Changing `chunk`'s partitioning.

## Notes

- Backend policy was settled at triage: shell out to external tools with an explicit
  availability check, rather than taking a Rust PDF-writing crate. The report's complaint was
  silence about a missing tool, not the dependency itself, and no Rust crate was identified
  that matches `qpdf` on resource subsetting — which is the whole point of preferring it.
- Tools used in the original session: `pdfinfo`, `pdftotext`, `pdfseparate`, `pdfunite`
  (poppler-utils). `qpdf` and `pdftk` were both absent on that machine, so the absent-`qpdf`
  path is the one that will actually run there — make sure its warning is good.
- `pdf-extract 0.7` (`apps/cli/Cargo.toml:24`) is currently the only PDF dependency, and
  nothing in the repo shells out to poppler today. This task introduces the first external
  process dependency, which is why every invocation must probe and report rather than assume.
- Whether this task is worth doing at all was left open at triage, on the grounds that
  `feedback-page-addressable-text.md` takes its most-wanted item. What survives is genuinely
  useful — `info` answers "is this splittable?" before any storing, and `extract` is the only
  way to get a chapter under its own role — but it is the lowest-value of the three book tasks
  and the only one that adds external dependencies. Reasonable to defer.
- Discovered loading DeLanda, *A New Philosophy of Society* (`openalex:W3027234447`) — 147
  pages, born-digital InDesign, 141 pages with a text layer.
