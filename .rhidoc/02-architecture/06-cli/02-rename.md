---
title: Rename
summary: The braincrawl rename command gives a work file on disk one canonical bibliographic filename — Surname[EtAl]Year-title-slug.ext. The store itself never reads filenames; the name serves the human archive, applied at import time.
tags: [cli, rename, filename, naming, library]
deps: [doc02.01.01]
---

# Rename

`braincrawl rename` renames a work file on disk to its canonical bibliographic
filename. The Library (doc02.01.01) never reads filenames — `library put` stores
bytes under the work's UUID and role — so the canonical name serves the human
archive: a downloads folder or shared drive stays greppable by the same
author and year the Catalog knows.

## The format

```
{Surname}[EtAl]{Year}-{title-slug}.{ext}
```

- **Surname** — the first author's surname, capitalized, particles folded in
  (`Van De Mieroop` → `VanDeMieroop`).
- **EtAl** — present exactly when the work has more than one author
  (`--et-al`).
- **Year** — the publication year.
- **title-slug** — the title lowercased, diacritics folded to ASCII,
  stopwords dropped, tokens hyphen-separated, truncated to a bounded length.
- **ext** — the original file extension, preserved.

The command is the format's source of truth: the exact slug rules
(stopword list, truncation bound, collision suffixing) live in
`apps/cli/src/rename.rs` and its tests, not here.

## Examples

```
Delanda2006-new-philosophy-society-assemblage-theory.pdf
NicholsonEtAl2018-everything-flows-towards-processual-philosophy.pdf
Dupre2025-everyone-flows-process-philosophy-human.pdf
```

## When to rename

Rename at import time — in the same pass that runs `library put` — so every
file that enters the Library leaves a canonically named copy behind. Use
`--dry-run` to preview the target path without renaming.
