# Consolidate the work-summary projection in chat tools

## Motivation

Tool returns are the largest context sink and the main token-economy lever a
harness has. In `web/src/plugins/chat/tools.ts` the same result projection —
`results.map((r) => ({ id, title, year, authors: extractAuthorships(r) }))` —
is hand-written five times (`openalexSearch`, `openalexCitedBy`, `openalexRefs`,
`openalexFind`, and a superset in `openalexGet`). Tuning what a work summary
carries (add a field, cap a list, offer concise-vs-detailed) is currently a
five-place edit. Fold it into one helper so token-economy changes are a single
edit and every tool stays consistent for free.

## Do NOT

- Do NOT change any tool's on-the-wire return shape or field names. This is a
  pure refactor: the JSON a tool returns must be byte-identical before and after,
  so existing behavior and any snapshot expectations hold.
- Do NOT touch the loop, the wire types, the transport, or the display code —
  only `tools.ts` (and a new test file).
- Do NOT fold `pushWork` / catalog-push calls into the summary helper. That is a
  separate concern (writing to the store, not shaping the return) and stays where
  it is. Leave the `Promise.all(results.map((r) => pushWork(...)))` calls alone.
- Do NOT add pagination, a detailed/concise mode, or a size cap in this phase —
  that is deliberately deferred (see Out of Scope). This phase only removes the
  duplication of the *current* shape.

## Plan

### 1. Add a `summarizeWork` / `summarizeWorks` helper

Near the existing `extractAuthorships` helper in `tools.ts`, add:

```ts
function summarizeWork(record: Record<string, unknown>): {
  id: unknown;
  title: unknown;
  year: unknown;
  authors: string[];
} {
  return {
    id: record.id,
    title: record.display_name ?? record.title,
    year: record.publication_year,
    authors: extractAuthorships(record),
  };
}

function summarizeWorks(records: Record<string, unknown>[]) {
  return records.map(summarizeWork);
}
```

Match the exact field order and values used today (`display_name ?? title`) so
output is unchanged.

### 2. Replace the four identical projections

In `openalexSearch`, `openalexCitedBy`, `openalexRefs`, and `openalexFind`,
replace the inline `const summary = results.map((r) => ({ ... }))` with
`const summary = summarizeWorks(results)`. Keep the surrounding `ok({...})`
wrappers and their other fields (`query`, `entity`, `count`, `work_id`,
`filters`) exactly as they are.

### 3. Reuse the base shape in `openalexGet`

`openalexGet` returns the four common fields plus `venue`, `cited_by_count`, and
`abstract`. Build it as `{ ...summarizeWork(record), venue, cited_by_count, abstract }`
so the shared fields come from the one helper and the extra fields are added
locally. Confirm the resulting object has the same keys and values as before.

### 4. Add a focused unit test

Create `web/src/plugins/chat/tools.summary.test.ts`. Import the module and test
the summary helper's shape against a small fixed OpenAlex-style record (with
`display_name`, `publication_year`, `authorships`). If `summarizeWork` is not
exported, export it (and `summarizeWorks`) — exporting internal helpers for test
is fine here. Assert the exact `{ id, title, year, authors }` shape and that
`title` falls back to `display_name` over `title`.

## Files to Modify

- `web/src/plugins/chat/tools.ts` — add `summarizeWork`/`summarizeWorks`; replace
  five inline projections; export the helper(s) for test.
- `web/src/plugins/chat/tools.summary.test.ts` — new; unit-test the helper shape.

## Verification

```bash
just web-test
just web-check
```

## Out of Scope

- Pagination, size caps, or a concise-vs-detailed return mode (a later token-economy
  pass, once measurement exists).
- Consolidating the repeated `pushWork` catalog-push calls.
- Any change to `prompt.md` or tool descriptions (that is the next chain phase).

## Notes

- The whole point is that the return shape now has one definition. A reviewer
  should confirm no tool's output changed — diff the `ok(...)` payloads mentally
  against the originals.

## Surface after this phase

- `web/src/plugins/chat/tools.ts` exports `summarizeWork(record)` and
  `summarizeWorks(records)` producing the `{ id, title, year, authors }` shape;
  all five OpenAlex tools build their work summaries through them.
- Every tool's return JSON is unchanged from before this phase — same keys, same
  values, same order.
- The tool registry array, `findTool`, `ToolDefinition`/`Tool`/`ToolResult`
  types, `truncate`, `ok`, `fail`, `pushWork`, and `extractAuthorships` are all
  unchanged and can still be relied on.
- `prompt.md` is deliberately untouched — its `## Tools` section still duplicates
  the registry descriptions (the next phase resolves that).
