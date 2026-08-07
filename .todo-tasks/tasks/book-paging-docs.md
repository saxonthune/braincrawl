# Bring the reading-guide docs in line with `paginate` / `outline` / `read`

## Motivation

Three documents currently instruct the opposite of what the first three phases of this chain
build. They are what a reading-guide session actually reads, so leaving them stale would steer
every session straight back into the workflow the chain exists to end.

`.claude/skills/braincrawl/reading-guide.md` (symlinked as `~/.claude/skills/braincrawl/`, so
editing it in-repo is the whole job) says:

> Read the passage itself. `pdftotext -f <first> -l <last> -layout <file>` on the PDF pages
> around the question is the direct route

and, under "Record the printed-page to PDF-page offset":

> Establish the offset once, from a page the user names or from the table of contents, and put
> it in the doc as a `#reference` node

That is a single global offset — the exact assumption phase 2 removes by recording a folio per
page — and a instruction to shell out to poppler on a loose file rather than read the store.

`.claude/skills/braincrawl/SKILL.md` §4 ("Read by progressive disclosure", line 199) describes
the read ladder as ending at "read the whole work" via `library get --role text`. There is now a
rung between condensed summary and whole text: address a place in the book directly.

`.rhidoc/01-product/01-glossary.md:49` says an artifact's role "says what the content is to the
work". After phase 3 that is no longer the whole story — role says what the content is, and
`derived_from` says which artifact it came from, so a work can hold several roots.

## Do NOT

- Do **not** narrate the change. No "previously", no "as of", no deprecation notes, no
  changelog paragraphs. These docs state current truth; the history is in git.
- Do **not** add the new verbs to `SKILL.md` as an exhaustive CLI inventory. That file already
  says it "names verbs by way of example, not as an exhaustive or current inventory" and tells
  the reader to run `--help`. Keep that posture.
- Do **not** invent glossary terms. `folio`, `outline`, `paginate`, and `page span` are the
  terms this chain settled on; use those and no others.
- Do **not** rewrite the Research Document node grammar, or change what a `#reference` node is
  for. It still carries the edition map — it just stops carrying an offset to do arithmetic with.
- Do **not** touch the existing L3 documents in the `braincrawl-l3` store. Their `#reference`
  nodes describe editions accurately and are not this task's business.

## Plan

### 1. `reading-guide.md` — replace the two stale sections

Replace "Record the printed-page to PDF-page offset" with a section on deriving the page and
outline artifacts once, at ingest: `library paginate` after `extract-text`, then `library
outline` from the table of contents read off the book. State that folios are recorded per page
and that `paginate` reports the pages where it could not find one, so the `#reference` node
records *which edition is cited*, not an offset.

Replace the `pdftotext` line under "Answering a question" with `library read`, showing the three
address forms — `--printed`, `--section`, `--find` — and noting that its output carries page
markers so a quote can be cited without a second lookup.

Keep the rest of the file as it stands: one question one finding, the glossary-is-one-node rule,
and the document shape all survive unchanged.

### 2. `SKILL.md` §4 — add the rung

One entry between "AI-condensed summary" and "Full text": addressing a place in the book
directly, with `library read`, once `paginate` and `outline` have been derived. Match the
existing entries' register — what it costs and what it tells you.

Also add one line to §4's `library list` sentence noting it now shows which artifacts derive
from which, so a session can see that a work holds two editions before it picks one.

### 3. Glossary — role and lineage

Amend `01-glossary.md:49-53`, in the existing verbalized-fact style:

- role says what the content is; `derived_from` says which artifact it was derived from
- a work may hold more than one root artifact — two editions of the same book, each with its
  own derivations
- add `folio` as a term: the printed page number as it appears on the page, recorded per page,
  never computed from an offset

## Files to Modify

- `.claude/skills/braincrawl/reading-guide.md` — the two sections above
- `.claude/skills/braincrawl/SKILL.md` — §4 read ladder and the `library list` sentence
- `.rhidoc/01-product/01-glossary.md` — role, lineage, `folio`

## Verification

```bash
cargo test
rhidoc manifest
```

## Out of Scope

- Any code change. Every verb this task documents exists after phases 1-3.
- The L3 documents in `braincrawl-l3`.
- `feedback-when-to-record-vs-auto-collect`, which is about when a session records at all —
  a separate inbox draft.

## Notes

- `rhidoc manifest` is in the verification block to catch a malformed glossary edit. If the
  workspace has no manifest step that exits non-zero on a doc error, drop it and rely on
  `cargo test` — do not invent a doc linter for this task.
- The `#reference` node's per-section role table (see `delanda-new-philosophy-society.l3.md`)
  is the hand-written form of what `library outline` now stores. Mention that the outline is
  authored from the table of contents; do not go back and convert existing documents.

## Surface after this phase

- `reading-guide.md` instructs `library paginate` / `library outline` at ingest and
  `library read` when answering, and no longer instructs `pdftotext` or a stored global offset.
- `SKILL.md` §4 carries a direct-addressing rung and notes that `library list` shows lineage.
- `01-glossary.md` defines `folio`, and states that role and `derived_from` are separate facts
  and that a work may hold several root artifacts.
- No code, test, or schema changes in this phase.
