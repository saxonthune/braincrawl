# Web UI: extended agent tool inventory (CLI parity)

## Motivation

The agent loop shipped with six core tools. This task adds the rest of
the inventory that desktop braincrawl sessions actually use, so the
phone agent reaches CLI parity for gathering and survey work.

## Do NOT

- Do NOT restructure the existing tools module — extend the merged
  `Tool[]` registry in `web/src/plugins/chat/tools.ts` following its
  exact `ToolDefinition`/`ToolHandler`/`Tool` shapes.
- Do NOT change the six existing tools' names or schemas.
- Do NOT add fulltext acquisition (fetch-content/push/extract) — desktop
  concern until the worker grows jobs.
- Do NOT add providers beyond OpenAlex.

## Plan

### 1. New tools (same registry, same result conventions)

1. `openalex_refs(work_id, limit=15)` — backward references
   (`/works?filter=cited_by:<id>` — verify the correct OpenAlex filter
   the CLI uses for refs and mirror it); push works + edges to the store
   like `openalex_cited_by` does.
2. `openalex_get(id, with_abstract=true)` — single entity; when the
   abstract inverted index is present, reconstruct the abstract text
   (OpenAlex stores `abstract_inverted_index`; invert it). Compact
   projection: title, year, authors, venue, cited_by_count, abstract.
3. `openalex_find(filters, entity="works", limit=15)` — raw
   `filter=` string pass-through for topic-gated expansion
   (e.g. `cites:W...,concepts.id:C...`); push works to the store.
4. `openalex_autocomplete_topics(prefix)` — `/autocomplete/topics`
   (or `/autocomplete/concepts` — mirror the CLI's choice), for
   concept-id lookup used by topic-gating.
5. `works_have(ids)` — `POST /works/have` against the store; membership
   check before pulling.
6. `store_stats()` — `GET /stats`; corpus overview.
7. `l3_list_docs()` — `GET /api/l3/docs`.
8. `reading_list()` — client-side over `GET /api/l3/graph`: nodes
   carrying a `reading` property, grouped by role
   (start-here/core/rigor/reference), each with its `why` and linked
   catalog id — mirror the web reading-list plugin's extraction logic
   (reuse/share its helper if cleanly importable).

### 2. Prompt additions

Extend `web/src/plugins/chat/prompt.md`'s Tools section with one line
per new tool stating WHEN to reach for it (triggering guidance, not just
description): refs for modern works' bibliographies; find+autocomplete
as the citation-scatter control (gate forward expansion by concept);
works_have before any pull; reading_list and l3_list_docs for
survey-the-collection questions.

### 3. Size discipline

Tool results stay compact (the existing truncation convention). The 14
tool definitions all ride the cacheable prefix — keep schemas terse
(short descriptions, no redundant properties).

## Files to Modify

- `web/src/plugins/chat/tools.ts` — eight new entries + shared OpenAlex
  helpers (abstract inversion, filter builder)
- `web/src/plugins/chat/prompt.md` — Tools section additions
- `web/src/plugins/reading-list.tsx` — only if extracting a shared
  helper for the reading-role grouping

## Verification

```bash
pnpm -C web install
pnpm -C web build
```

## Out of Scope

- fetch-content / extract-text / push / chunk
- UI changes beyond none (tools are model-facing)

## Surface after this phase

- The registry exposes 14 tools: the original six plus the eight above,
  names exactly as listed.
- prompt.md documents triggering rules for all of them.
- Negative space: existing six tools byte-identical; transport, store,
  chat UI untouched.
