# Give each tool one documentation home

## Motivation

A tool's description currently lives in two places: the model-facing `description`
string in the `tools` registry (`web/src/plugins/chat/tools.ts`) and the `## Tools`
prose in `web/src/plugins/chat/prompt.md` (lines ~39-79). They overlap and can
drift, which violates the one-fact-one-home rule. The registry description is the
real contract — it is what goes over the wire to the model (`transport.ts`
`toApiTools`). The prompt should hold only *cross-tool policy* the schema cannot
carry. This phase makes the registry the single home for "what a tool is" and
trims `prompt.md` to behavior that spans tools.

## Do NOT

- Do NOT lose any guidance. Every actionable note in `prompt.md`'s current
  `## Tools` list must survive — either moved into the relevant tool's registry
  `description` (if it is per-tool "when / when-not / caution") or kept in a
  cross-tool policy section of the prompt (if it is behavior across tools). Nothing
  is deleted outright.
- Do NOT change tool names, schemas, parameters, handlers, or return shapes. Only
  `description` strings in the registry and prose in `prompt.md` change.
- Do NOT touch the loop, transport, display, store, or `tools.ts` logic other than
  the `description` fields.
- Do NOT remove the prompt's non-tool sections (`The store`, `Session shape`,
  `Research documents (L3 grammar)`, `Recording`, `Style`) — those are policy and
  stay.

## Plan

### 1. Classify each note in `prompt.md`'s `## Tools` list

For each tool bullet in `prompt.md` (lines ~39-79), split its content into:
- **Per-tool contract** — what the tool is, when to use it, when NOT to, its
  failure modes. Examples: `openalex_cited_by`'s citation-scatter caution ("gate
  by topic/concept when expanding"); `read_pages`'s "pages are book pages unless
  the user says otherwise; calibrate the offset via headings"; `openalex_refs`'s
  "reach for this on a recent work whose forward citations are thin";
  `openalex_get`'s "check an abstract before deciding whether to pull further";
  `works_have`'s "call before pulling a batch". These move into that tool's
  registry `description`.
- **Cross-tool behavior** — the answer-order ladder ("Answer at the cheapest step
  that suffices: docs → catalog → abstracts → page text; name the gap before
  reaching outward; verification is a lens, never a gate"). This is NOT about one
  tool; keep it in the prompt.

### 2. Enrich the registry descriptions

In `tools.ts`, extend each tool's `description` with the per-tool guidance from
step 1, written as onboarding prose (purpose, when to use, when not to, known
failure modes). Keep them tight — a few sentences. The existing descriptions are
the starting point; augment, don't rewrite from scratch. Especially:
- `openalex_cited_by` — add the citation-scatter caution and the gate-by-concept
  advice.
- `openalex_refs` — add "prefer this over cited_by for a recent work with thin
  forward citations."
- `openalex_get` — add "use to check an abstract/profile before pulling a hit
  further."
- `read_pages` — add the book-page vs chunk-page calibration note.
- `openalex_find` / `openalex_autocomplete_topics` — add that they pair up as the
  citation-scatter control (gate `find` by a concept id from `autocomplete`).
- `works_have` — already says "before pulling"; keep.

### 3. Replace the prompt's `## Tools` list with a cross-tool policy section

Remove the per-tool enumeration from `prompt.md`. Replace the `## Tools` section
with a short section (retitle to `## Working across tools` or keep `## Tools` but
make it policy-only) that keeps ONLY the cross-tool ladder and cautions: answer at
the cheapest step that suffices (docs → catalog → abstracts → page text), name the
gap before reaching outward, verification is a lens not a gate, and the general
citation-scatter warning as a cross-cutting principle. Do not list individual
tools or their signatures — the model receives those from the schema.

### 4. Sanity-check nothing was dropped

Diff the old `## Tools` bullets against (registry descriptions + new policy
section). Every clause should map to one side. Note any that don't in the result.

## Files to Modify

- `web/src/plugins/chat/tools.ts` — enrich `description` strings with per-tool
  guidance migrated from the prompt.
- `web/src/plugins/chat/prompt.md` — remove the per-tool enumeration; keep a
  cross-tool policy section.

## Verification

```bash
just web-test
just web-check
```

## Out of Scope

- Any schema/parameter/handler change.
- Restructuring the other prompt sections.
- Building a generator that emits the tool list from the registry (not needed once
  the prompt no longer enumerates tools).

## Notes

- The test suite will not catch dropped guidance — this phase's real check is the
  step-4 mapping. The result note should state, per migrated clause, where it
  landed (a description, or the policy section).
- Keep descriptions token-conscious: they are sent on every turn for every tool.
  Onboarding-doc quality, not an essay.

## Surface after this phase

- Each tool's registry `description` in `tools.ts` is the single model-facing
  home for what that tool is and when to use/avoid it; per-tool cautions
  previously only in `prompt.md` now live there.
- `prompt.md` no longer enumerates individual tools; it retains a cross-tool
  policy section (answer-order ladder, gap-naming, verification-as-lens,
  citation-scatter as a cross-cutting principle) and all non-tool sections
  (`The store`, `Session shape`, `Research documents`, `Recording`, `Style`).
- Tool names, schemas, parameters, handlers, and return shapes are unchanged from
  the previous phase; `summarizeWork`/`summarizeWorks` and the registry array
  shape still hold.
