# Make the agent loop testable through an injectable ModelCall seam

## Motivation

The principles call for measuring the harness with a scripted stand-in model so
the loop and tools can be exercised without a network or a real model. braincrawl
already has the right seam type — `ModelCall` — but it is not injectable: `runLoop`
in `web/src/plugins/chat/transport.ts` resolves the `ModelCall` internally from
settings (`resolveModelCall(getSetting(...), ...)`), and `runLoop` is not exported.
So today nothing can drive the loop deterministically. This phase makes `ModelCall`
injectable, adds a scripted-model helper, and writes golden-turn tests that assert
the reason/act/observe cycle. The loop is the riskiest code and currently has zero
tests (only `toDisplayItems` is covered).

## Do NOT

- Do NOT change the production behavior of `sendTurn`/`resumeTurn`. When no
  `ModelCall` is injected, `runLoop` must resolve from settings exactly as it does
  now, including the invalid-slug note-message path.
- Do NOT move the loop server-side, change the wire types, or change tool schemas
  or handlers. This phase adds a seam and tests; it does not restructure the loop.
- Do NOT hit the real network in tests. Mock `storeFetch` (and `fetch` where a tool
  reaches OpenAlex directly) so golden tests are deterministic and offline.
- Do NOT weaken `MAX_ITERATIONS`, the resume logic, or the `pendingTurn` handling.

## Plan

### 1. Make `ModelCall` injectable into `runLoop`

In `transport.ts`, add an optional third parameter:
`runLoop(sessionId: string, onDelta: (text: string) => void, modelCallOverride?: ModelCall)`.
When `modelCallOverride` is provided, use it directly and skip `resolveModelCall`
(and its settings reads and note-return branch). When it is absent, keep the
current path unchanged (resolve from settings; return the note message on an
invalid slug). Thread it through so `sendTurn` and `resumeTurn` call `runLoop`
with no override (production unchanged).

Export `runLoop` so tests can call it. `sendTurn`/`resumeTurn` keep their current
signatures.

### 2. Add a scripted-model helper

Create `web/src/plugins/chat/testing/scriptedModel.ts` exporting
`scriptedModelCall(turns)`, where `turns` is an ordered list of
`{ content: ContentBlock[]; stopReason: "tool_use" | "end_turn" | "max_tokens" | "refusal" | "other"; text?: string }`.
It returns a `ModelCall` that, on each invocation, yields the next scripted turn
(and, if `text` is given, calls `onText` with it once to simulate streaming). It
throws if called more times than there are scripted turns — an over-run means the
loop did not stop when the script expected.

### 3. Write golden-turn tests

Create `web/src/plugins/chat/harness.golden.test.ts`. Import test helpers from
`vite-plus/test` (mirror how `display.test.ts` imports; use `vi` from the same
module for mocking). For each golden case: `vi.mock` the store service module
(`../../services/store`) so `storeFetch` returns canned `Response`s; set up a
session via `createSession` + `appendMessage` (a user message); run `runLoop` with
a `scriptedModelCall`; assert the resulting session messages.

Cover at least:
- **Single tool round-trip.** Scripted turns: (1) `tool_use` for `store_stats`,
  (2) `end_turn` text. With `storeFetch` mocked to return canned stats JSON, assert
  the message sequence is assistant(tool_use) → user(tool_result matching the
  canned payload) → assistant(text), and that `store_stats` produced a
  non-error `tool_result`.
- **Unknown tool.** Scripted `tool_use` for a name not in the registry; assert the
  loop appends a `tool_result` with `is_error: true` and `"unknown tool"` content,
  then completes on the next `end_turn` turn.
- **Iteration cap.** A scripted model that emits `tool_use` every turn; assert the
  loop stops at `MAX_ITERATIONS` with the cap note appended and does not loop
  forever.

### 4. Add a `just web-eval` recipe

In `justfile`, add a `web-eval` recipe next to `web-test` that runs only the
golden file, e.g. `cd {{justfile_directory()}}/web && vp test golden` (a filter
that matches `harness.golden.test.ts`). Confirm the filter actually selects the
file. This is the "run the fixed set" entry point future prompt/tool changes will
use.

## Files to Modify

- `web/src/plugins/chat/transport.ts` — add the injectable `modelCallOverride`
  param to `runLoop`; export `runLoop`; keep `sendTurn`/`resumeTurn` production
  behavior unchanged.
- `web/src/plugins/chat/testing/scriptedModel.ts` — new; `scriptedModelCall`.
- `web/src/plugins/chat/harness.golden.test.ts` — new; the golden-turn tests.
- `justfile` — new `web-eval` recipe.

## Verification

```bash
just web-test
just web-check
just web-eval
```

## Out of Scope

- A fixture-file replay harness (recorded `.turns.json` on disk) — the scripted
  helper plus in-test cases are the borrowable core; disk fixtures are a later
  design pass.
- Asserting on L3 doc-write side effects beyond tool_result content.
- Any change to real provider selection in `resolveModelCall`.

## Notes

- The store uses `localStorage`; the test environment already supports it
  (`display.test.ts` runs). Ensure each test starts from a clean session (fresh
  `createSession`) to avoid cross-test bleed through `localStorage`.
- `runLoop` writes streamed text via `updateLastAssistantText` and finalizes via
  `finalizeAssistantMessage`; assert against the finalized session messages, not
  intermediate streaming state.
- Keep `scriptedModel.ts` under a `testing/` folder so it reads as test support,
  not production code, while still being importable and reusable.

## Surface after this phase

- `runLoop(sessionId, onDelta, modelCallOverride?)` is exported from
  `transport.ts`; passing a `ModelCall` bypasses settings resolution, passing none
  preserves current production behavior. `sendTurn`/`resumeTurn` signatures and
  behavior are unchanged.
- `web/src/plugins/chat/testing/scriptedModel.ts` exports `scriptedModelCall(turns)`
  returning a deterministic `ModelCall` over a fixed turn script.
- `web/src/plugins/chat/harness.golden.test.ts` exercises the loop (tool
  round-trip, unknown tool, iteration cap) offline.
- `just web-eval` runs the golden set.
- Tool return shapes (`summarizeWork`) and the single-home tool descriptions from
  the prior phases are unchanged and relied upon by the golden assertions.
