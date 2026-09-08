# PDF import — a local file becomes a readable, queryable work

Read this file when the session's job is to bring one or more local PDF files
(a downloads folder, a shared drive) into the Library and Catalog. The outcome
per file is: a canonical Catalog record, the bytes stored as `fulltext`, a
`text` artifact, a `pages` artifact with trusted folios, and a canonically
renamed source file. The shared machinery (server, CLI, layers) is in
`SKILL.md`; read that first.

This is a **PDF-import** workflow. Begin with the local file and its title page. A Research
Collection survey is optional and belongs after import unless the user explicitly asks for the
PDF to be integrated into an existing research line. The Catalog identity check in §2 is still
required because it prevents duplicate work records before bytes are stored.

## The books-articles archive (source and destination)

The human PDF archive lives at `~/Documents/books-articles/`. Its `drop-zone/`
subdirectory holds works **staged for import**; the parent holds the **finished** works —
those already keyed in the store, canonically named. A PDF-import session, by default:

1. reads each file in `~/Documents/books-articles/drop-zone/`,
2. runs the import below (identify → land record → put/extract/paginate/anchor),
3. renames the file (§5) and moves it up into `~/Documents/books-articles/`,
4. leaves drop-zone empty of finished works.

A file belongs in the parent once the store holds its `fulltext`, `text`, and `pages`. That
directory carries its own `CLAUDE.md` stating the same contract.

## 1. Identify the work — read the PDF, never trust the filename

Open the PDF's first pages (title page, copyright page) before anything else.
Filenames lie: an ISBN-named file or a mangled libgen name routinely turns out
to be a different work, a different edition, or an edited volume. From the
title page take: title, author(s) or editor(s), publisher, year, and any DOI
or ISBN printed there or embedded in the filename.

## 2. Land the canonical Catalog record

**First ask what the store already holds.** A prior session may have landed this
work under a *different* provider record — a different edition's DOI, even a
book-review record used as the book's citable proxy. Landing a second record
forks the work: two store nodes, disjoint aliases, and eventually duplicate
fulltexts that union-find cannot merge (no shared strong id). So before any
provider lookup:

```bash
braincrawl --text catalog search --title "<distinctive title words>"
braincrawl --text catalog search --author <surname-substring>
```

If a node for the work exists, attach artifacts to *its* alias and stop here.
(Search author names generously: `landa` finds both "DeLanda" and "De Landa";
accented names need an unaccented substring.)

Otherwise look the work up — the lookup itself pushes the record to the store:

```bash
braincrawl --text --limit 5 openalex search works "<title>"
braincrawl --text openalex get doi:<doi>          # when a DOI is in hand
```

**Expect duplicates and works-about-the-work.** For a cited book, OpenAlex
usually holds several records: the book, a re-listing of it, and book reviews
titled like it. Pick the record the PDF actually is, checking in order:

- `type` — prefer `book` (or the right type) over `article`;
- `doi` — a review's DOI points at a journal (e.g. a `10.5840/…` Process
  Studies id on a candidate that outscored the real book on citations);
- `cited_by_count` — among true duplicates, take the heavily cited one;
- `authorships` — a review's author is the reviewer, not the book's author.

**No provider record at all** (a book with only an ISBN): mint the node by
hand, then continue with the alias you minted:

```bash
braincrawl catalog add --alias isbn:<isbn> --title "…" --author "…" --year <y>
```

## 3. Store and derive artifacts

```bash
braincrawl --text library put <id> "<file.pdf>" --source "user-pdf"
braincrawl --text library extract-text <id>
braincrawl --text library paginate <id>
```

`<id>` is the canonical record's alias (`openalex:W…`, `isbn:…`). The PDF
extractor prints font-glyph warnings to stderr — noise, not failure; judge the
run by its final summary line. `library list <id>` should then show `fulltext`
with `text` and `pages` derived from it.

### When the extractor cannot read the PDF

`extract-text` or `paginate` may report "could not extract text from this PDF"
(the built-in extractor cannot parse some font/page structures). It fails
cleanly and leaves the rest of the import intact. Recover by extracting the text
externally and paginating from it:

```bash
pdftotext "<file.pdf>" book.txt          # keeps the default form-feed page breaks
braincrawl --text library put <id> book.txt --role text
braincrawl --text library paginate <id> --from text
```

`paginate --from text` splits on the form-feed character `pdftotext` writes at
each page boundary, so the page grid matches the PDF's pages and folio
anchoring works unchanged. If the text carries a different page-break
convention, declare it with `--separator`: `blank-lines:<n>`, or
`regex:<pattern>` to start a page at each line matching the pattern.

## 4. Anchor the folios — printed pages must be trusted, not guessed

`paginate` auto-detects printed page numbers (folios). Read its summary: many
"page(s) with no folio" means printed-page addressing is unreliable. Fix it
with an anchor, derived and verified — never assumed:

1. Take two auto-detected pairs (`artifact-page 99 = folio 92`,
   `artifact-page 125 = folio 118`) and confirm they imply one constant offset.
2. Locate printed page 1 by that offset and confirm by reading it:
   `braincrawl --text library read <id> --artifact-page <n>` — the page must
   show the body's opening (an introduction's first page), and page `<n-1>`
   front matter.
3. Declare it: `braincrawl library paginate <id> --anchor <page_index>=1 --force`
4. Verify one printed read resolves: `library read <id> --printed 92`.

For essay collections, also consider a hand-authored table of contents
(`library outline`), which makes `library read --section` work.

## 5. Rename the source file

In the same pass, rename the file to its canonical bibliographic name —
`{Surname}[EtAl]{Year}-{title-slug}.{ext}`, spec'd in `doc02.06.02`
(`.rhidoc/02-architecture/06-cli/02-rename.md`); the command is the format's
source of truth:

```bash
braincrawl rename --author <Surname> [--et-al] --year <y> --title "…" "<file.pdf>"
```

Use `--dry-run` to preview. The store does not care about filenames — the
rename keeps the human archive greppable by the author and year the Catalog
knows.

## 6. Verify the import

```bash
braincrawl --text catalog search --author <surname> --with-artifact fulltext
braincrawl --text library read <id> --find "<a phrase the work must contain>"
```

The first proves the work is findable as a held PDF; the second proves the
paged index resolves to real text. Author matching is ASCII-case-insensitive
only, so search accented names by an unaccented substring (`dupr`, not
`dupre`, finds Dupré).

## Reading from a paginated source

The point of the pages artifact: reach a place in the work directly, with
page markers fit for citation, instead of condensing the whole text.

```bash
braincrawl --text library read <id> --printed 9-12     # printed folio range
braincrawl --text library read <id> --artifact-page 16 # page as the source artifact orders it
braincrawl --text library read <id> --section ch01     # needs an outline artifact
braincrawl --text library read <id> --find "assemblage" # quote search
```

Quote page numbers from the `=== p.N ===` markers in the output; after
anchoring (§4) they are printed folios, safe to cite.
