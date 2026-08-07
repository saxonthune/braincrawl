# `library list` returns 404 for a work that demonstrably holds artifacts

## Motivation

Starting a reading-guide session, I ran `braincrawl library list openalex:W3027234447` to
check what the store already held for DeLanda's *A New Philosophy of Society*. It returned
`error: server returned 404: not found`, so I reported to the user that the book was not in
the Library at all, and re-ran the whole ingest (`library put`, `extract-text`, `chunk`).

Every one of those write commands resolved the same alias without complaint, and `chunk`
then told me `already-present: chunks artifact already in store`. The artifacts had been
there the entire time — `fulltext`, `text`, `chunks`, and the per-section
`fulltext-intro` / `fulltext-ch01` … roles all read back fine through `library get`.

The cost is that `library list` is exactly the command the skill tells you to run when you
are unsure what a work holds, and its failure mode is indistinguishable from an empty
store. I gave the user a wrong answer about their own library and did redundant work
against the shared store on the strength of it.

## Description

`braincrawl library list <ns:id>` returns HTTP 404 for a work whose artifacts are present
and readable. Reproduced with `openalex:W3027234447`:

- `braincrawl --text library list openalex:W3027234447` → `error: server returned 404: not found`
- `braincrawl library get openalex:W3027234447 --role text` → returns the extracted text
- `braincrawl library get openalex:W3027234447 --role chunks` → returns the chunks JSON
- `braincrawl library get openalex:W3027234447 --role fulltext-ch01` → returns a PDF
- `braincrawl library put openalex:W3027234447 <file> --role fulltext` → `pushed: 3142483 bytes`
- `braincrawl library chunk openalex:W3027234447 --allow-partial` → `already-present`

So the alias resolves for `get`, `put`, and `chunk`, but not for `list`. Also worth noting:
`braincrawl --text catalog get openalex:W3027234447` printed a row of empty fields (`-` and
a tab, nothing else), which did not help me tell whether the work or the artifacts were the
missing thing.

Separately from the bug, a 404 is a confusing shape for this answer even when it is
correct — "this work holds no artifacts" and "I have never heard of this work" are
different situations, and as a user I need to tell them apart before deciding whether to
ingest.

## Scope

- `library list` resolves a work by the same alias path the other `library` subcommands use
- The empty-vs-unknown distinction is legible from the output

## Out of Scope

- The per-section role convention itself (`fulltext-ch01` etc.)
- `catalog get`'s empty `--text` projection — related friction, but a separate report if
  it turns out not to share a cause

## Resolution

The suspicion was wrong — all verbs share one alias-resolution path. The 404 came from
the deployed Worker at `bcp.saxon.zone` predating commit `c771942` (2026-07-22), which
added `/works/*id/artifacts` to both servers; the request fell through to the catch-all
404 at `apps/worker/src/lib.rs:629`. Confirmed by probing with a path-unsafe role, which
current code answers 400 and the deployed build answers 404. The deploy is outstanding.

Fixed in `43f4c6e`: `library list` now reports an unknown work distinctly from a work
holding zero artifacts, and the `--text` renderer falls back to `canonical_id` and
`attrs.title`, which also fixes `catalog get --text` printing a bare `-`.

## Notes

- Suspicion, held loosely: `list` may be looking the work up by canonical UUID directly
  while `get`/`put` go through alias resolution first. I did not read the source.
- Encountered on 2026-07-27 against the systemd user service at `127.0.0.1:8787`, store at
  `~/.local/share/braincrawl/`.
