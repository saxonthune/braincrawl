# Reading guide sessions

Read `SKILL.md` first — it holds the three layers, the CLI surface, and the Research Document
node grammar. This file covers only what a reading-guide session does differently.

A reading-guide session has one work at its centre. The user is reading it, or means to, and
asks questions about specific passages. You answer from the work's own text, not from the
Catalog. The Research Document that comes out is a record of *the user's questions and what
the text said in reply* — it is not a summary of the book.

## Get the work into the Library first

Rename to the canonical filename, push the bytes, derive the text and chunks:

```bash
braincrawl rename --author <Surname> --year <YYYY> --title "<Title>" [--et-al] <file>
braincrawl library put <ns:id> <renamed-file> --role fulltext --source "user-supplied"
braincrawl library extract-text <ns:id>
braincrawl library chunk <ns:id>
```

If `chunk` refuses because some pages carry no text layer, re-run with `--allow-partial` and
report which pages were skipped. A born-digital PDF splits cleanly; a scanned one does not,
and there is no OCR in the pipeline.

## Record the printed-page to PDF-page offset

Front matter almost always shifts the two. Establish the offset once, from a page the user
names or from the table of contents, and put it in the doc as a `#reference` node together
with the chapter starts in both numberings. Every later citation depends on it, and it is
the one node that is worth writing before anyone asks.

## Answering a question

Read the passage itself. `pdftotext -f <first> -l <last> -layout <file>` on the PDF pages
around the question is the direct route; `braincrawl library get <ns:id> --role text` reads
back what the store holds. Quote what the work actually says.

**Cite a page for every claim, and quote wherever a quote will do the work.** A finding that
says what the author argues, without a page, is not usable later — the point of the document
is that the user can go back to the passage. Put the printed page in the remarks, in the
`[p.11]` form the other documents use.

## One question, one finding

The user asks a question; you write the question node and the finding that answers it. Do not
fan one question into several nodes covering context, stakes, and adjacent material they did
not ask about. If reading the passage turned up something else worth recording, say so in
chat and let the user decide whether it becomes a node.

The document should stay close to the user's questions. A reading guide that has grown past
what was asked has stopped being a record of the reading.

## Controlled vocabulary of the work

A reading guide may build up the work's own vocabulary — the terms the author defines and
uses in a fixed sense.

**The glossary is one node, not one node per word.** A single `#glossary` node holds the
whole vocabulary, one property line per term, and you edit that node as the reading advances.
Splitting terms across nodes buries the vocabulary in the document and hides which terms are
defined against each other.

Add a term when the user has read the passage that defines it or has asked about it, quoting
the author's own definition with its page. Do not write the glossary ahead of the reading.
Do not add a term because it appears in the table of contents, the index, or a passage nobody
has reached. An entry for a term the user has not met is a guess about what the author means
by it, recorded as if it were read.

## Document shape

Same node grammar as any Research Document (`doc02.01.04`). The nodes a reading guide tends
to carry:

- `#doc-intro` — the work and what this reading is for.
- `#landmark` — the work itself, with its `catalog` reference and a note that the Library
  holds the fulltext.
- `#reference` — the edition page map.
- `#question` — one per question the user asked, in their terms.
- `#finding` — the answer, with page and quotation, linked `supports` to its question.
- `#glossary` — one node for the whole vocabulary, one property line per term.
